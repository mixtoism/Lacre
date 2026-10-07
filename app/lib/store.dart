import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_local_notifications_linux/flutter_local_notifications_linux.dart';
import 'package:path_provider/path_provider.dart';

import 'l10n/l10n.dart';
import 'src/rust/api/p2m.dart' as p2m;

/// Canal con el código nativo de Android (P2mApp.kt): clave de datos,
/// servicio en segundo plano, avisos y abrir archivos.
const sistema = MethodChannel('p2message/sistema');

/// Qué dicen los avisos del sistema de un mensaje nuevo. Se ve en la
/// pantalla, en la de bloqueo y en el historial de avisos (D-035).
enum NoticeContent {
  /// Remitente y texto.
  full,

  /// Solo quién escribe: «Mensaje nuevo».
  sender,

  /// Solo «Tienes mensajes nuevos».
  none;

  String label(AppLocalizations l) => switch (this) {
    full => l.noticeFull,
    sender => l.noticeSender,
    none => l.noticeNone,
  };
}

/// Estado de la app: perfil, chats y avisos del nodo.
class Store extends ChangeNotifier {
  p2m.Profile? profile;
  List<p2m.ChatInfo> chats = [];
  p2m.Status? status;
  String? relay;

  /// Error al arrancar el nodo.
  String? startError;

  /// Avisos para mostrar como SnackBar.
  final notices = StreamController<String>.broadcast();

  /// Mensajes nuevos, para la pantalla del chat abierto.
  final messages = StreamController<(int, p2m.Message)>.broadcast();

  /// Descargas en curso: id del archivo → fracción descargada.
  final downloads = <String, double>{};

  /// Archivos descargados en esta sesión: id → ruta.
  final downloaded = <String, String>{};

  Timer? _statusTimer;

  /// Modo de energía (D-034): en Android, reposo con la app en segundo
  /// plano; mDNS solo con wifi.
  AppLifecycleListener? _lifecycle;
  bool _background = false;
  bool _wifi = true;

  /// Chats que hay que abrir (al tocar el aviso de un mensaje).
  final openChat = StreamController<int>.broadcast();

  /// Chat que se está viendo.
  int? viewing;

  /// En Linux, avisos del escritorio (org.freedesktop.Notifications).
  LinuxFlutterLocalNotificationsPlugin? _desktop;

  /// Contenido de los avisos; `null` hasta que la persona elige. Mientras
  /// tanto los avisos no dicen nada (como [NoticeContent.none]).
  NoticeContent? noticeContent;
  File? _noticeFile;

  /// Id del aviso sin contenido: uno para todos los chats.
  static const _generalNotice = -1;

  /// Con `lanOnly` (o `P2M_LAN=1`), sin Tor: solo red local, para pruebas.
  Future<void> start({String? dataDir, bool lanOnly = false}) async {
    // P2M_DATA permite arrancar varias instancias en el mismo equipo.
    final dir =
        dataDir ??
        Platform.environment['P2M_DATA'] ??
        '${(await getApplicationSupportDirectory()).path}/node';
    _noticeFile = File('$dir/avisos.txt');
    try {
      final v = _noticeFile!.readAsStringSync().trim();
      noticeContent = NoticeContent.values
          .where((c) => c.name == v)
          .firstOrNull;
    } catch (_) {}
    try {
      // En Android la clave para cifrar el estado sale del Keystore (D-033);
      // en el escritorio la busca el núcleo en el llavero del sistema.
      Uint8List? key;
      if (Platform.isAndroid) {
        key = await sistema.invokeMethod<Uint8List>('claveDatos');
      }
      profile = await p2m.start(
        dataDir: dir,
        lanOnly: lanOnly || Platform.environment['P2M_LAN'] == '1',
        dataKey: key,
      );
    } catch (e) {
      debugPrint('p2m arranque: $e');
      startError = '$e';
      notifyListeners();
      return;
    }
    p2m.eventStream().listen(_onEvent);
    if (Platform.isAndroid) {
      // El nodo sigue vivo con la app cerrada (D-032).
      sistema.setMethodCallHandler((call) async {
        switch (call.method) {
          case 'abrirChat':
            openChat.add(call.arguments as int);
          case 'red':
            _wifi = call.arguments as bool;
            _applyPower();
        }
      });
      _wifi = await sistema.invokeMethod<bool>('wifi') ?? true;
      _background = _isBackground(WidgetsBinding.instance.lifecycleState);
      _lifecycle = AppLifecycleListener(
        onStateChange: (s) {
          final bg = _isBackground(s);
          if (bg == _background) return;
          _background = bg;
          _applyPower();
        },
      );
      _applyPower();
      unawaited(
        sistema
            .invokeMethod('servicio')
            .then((r) => debugPrint('p2m servicio: $r'))
            .catchError((Object e) => debugPrint('p2m servicio: $e')),
      );
    }
    if (Platform.isLinux) {
      final n = LinuxFlutterLocalNotificationsPlugin();
      try {
        await n.initialize(
          settings: LinuxInitializationSettings(
            defaultActionName: systemL10n.open,
          ),
          // Al pulsar el aviso, abre su chat.
          onDidReceiveNotificationResponse: (r) {
            final chat = int.tryParse(r.payload ?? '');
            if (chat != null) openChat.add(chat);
          },
        );
        _desktop = n;
      } catch (e) {
        // Sin servidor de avisos: solo los de dentro de la app.
        debugPrint('p2m avisos: $e');
      }
    }
    _refreshChats();
    _startStatusTimer();
    notifyListeners();
  }

  void _startStatusTimer() {
    _statusTimer?.cancel();
    _statusTimer = Timer.periodic(
      const Duration(seconds: 5),
      (_) => refreshStatus(),
    );
    refreshStatus();
  }

  /// Sin pantalla (o con la app oculta) el nodo está en segundo plano.
  static bool _isBackground(AppLifecycleState? s) =>
      s == null ||
      s == AppLifecycleState.hidden ||
      s == AppLifecycleState.paused ||
      s == AppLifecycleState.detached;

  void _applyPower() {
    debugPrint('p2m energía: segundo plano=$_background wifi=$_wifi');
    try {
      p2m.setPower(background: _background, wifi: _wifi);
    } catch (e) {
      debugPrint('p2m energía: $e');
    }
    // El lock de multicast impide que el wifi filtre paquetes: solo
    // mientras mDNS esté activo.
    unawaited(
      sistema
          .invokeMethod('multicast', !_background && _wifi)
          .catchError((Object e) => debugPrint('p2m multicast: $e')),
    );
    if (_background) {
      // Nadie mira el estado: no despertar al nodo cada 5 s.
      _statusTimer?.cancel();
      _statusTimer = null;
    } else if (_statusTimer == null) {
      _startStatusTimer();
    }
  }

  void _onEvent(p2m.UiEvent ev) {
    switch (ev) {
      case p2m.UiEvent_FileProgress(:final id, :final done, :final total):
        if (downloads.containsKey(id)) {
          downloads[id] = total == 0 ? 1 : done / total;
          notifyListeners();
        }
        return;
      case p2m.UiEvent_Linked(:final handle):
        notices.add(systemL10n.deviceLinked(handle));
        profile = p2m.profile();
      case p2m.UiEvent_NewChat(:final index, :final name, :final request):
        final text = request
            ? systemL10n.chatRequestFrom(name)
            : systemL10n.newChatWith(name);
        notices.add(text);
        _notify(index, 'p2message', text);
      case p2m.UiEvent_Avatar():
        _avatars.clear();
      case p2m.UiEvent_ChatsChanged():
        break;
      case p2m.UiEvent_Message(:final index, :final message, :final local):
        messages.add((index, message));
        if (!local && !message.mine && !message.notice) {
          _notifyMessage(index, message);
        }
      case p2m.UiEvent_Removed():
        notices.add(systemL10n.expelled);
      case p2m.UiEvent_UnverifiedMember(:final name):
        notices.add(systemL10n.unverifiedMember(name));
      case p2m.UiEvent_RelayReady(:final addr):
        relay = addr;
    }
    _refreshChats();
  }

  /// Aviso del sistema si la app no está a la vista: en Android (D-032)
  /// y en Linux. En el escritorio, sin foco la app está `inactive`.
  void _notify(int index, String title, String text) {
    // A la vista, los avisos salen dentro de la app.
    if (WidgetsBinding.instance.lifecycleState == AppLifecycleState.resumed) {
      return;
    }
    if (Platform.isAndroid) {
      sistema
          .invokeMethod('aviso', {'id': index, 'titulo': title, 'texto': text})
          .catchError((_) => null);
    } else {
      _desktop
          ?.show(
            id: index,
            title: title,
            body: text,
            payload: index < 0 ? null : '$index',
            notificationDetails: const LinuxNotificationDetails(
              category: LinuxNotificationCategory.imReceived,
            ),
          )
          .catchError((Object e) => debugPrint('p2m aviso: $e'));
    }
  }

  Future<void> setNoticeContent(NoticeContent c) async {
    noticeContent = c;
    notifyListeners();
    try {
      await _noticeFile?.writeAsString(c.name);
    } catch (e) {
      debugPrint('p2m avisos: $e');
    }
  }

  void _notifyMessage(int index, p2m.Message m) {
    final chat = p2m.chats().where((c) => c.index == index).firstOrNull;
    if (chat == null) return;
    final content = noticeContent ?? NoticeContent.none;
    if (content == NoticeContent.none) {
      _notify(_generalNotice, 'p2message', systemL10n.youHaveNewMessages);
      return;
    }
    final name = chat.peer != null ? m.from.split('#').first : chat.name;
    if (content == NoticeContent.sender) {
      _notify(
        index,
        name,
        chat.request ? systemL10n.chatRequestNotice : systemL10n.newMessage,
      );
      return;
    }
    // De una solicitud no se enseña el contenido.
    if (chat.request) {
      _notify(index, 'p2message', systemL10n.chatRequestFrom(chat.name));
      return;
    }
    final from = m.from.split('#').first;
    final body = m.sticker != null
        ? systemL10n.sticker
        : m.file != null
        ? '📎 ${m.file!.name}'
        : m.text;
    if (chat.peer != null) {
      _notify(index, from, body);
    } else {
      _notify(index, chat.name, '$from: $body');
    }
  }

  /// Se está viendo el chat `index` (`null`: ninguno). Quita su aviso.
  void view(int? index) {
    viewing = index;
    if (index == null) return;
    if (Platform.isAndroid) {
      sistema
          .invokeMethod('quitarAviso', {'id': index})
          .catchError((_) => null);
      sistema
          .invokeMethod('quitarAviso', {'id': _generalNotice})
          .catchError((_) => null);
    } else {
      _desktop?.cancel(id: index).catchError((_) => null);
      _desktop?.cancel(id: _generalNotice).catchError((_) => null);
    }
  }

  /// Android: la app puede usar la red con la pantalla apagada.
  Future<bool> batteryFree() async =>
      Platform.isAndroid &&
      (await sistema.invokeMethod<bool>('bateriaLibre') ?? false);

  Future<void> askBattery() => sistema.invokeMethod('pedirBateria');

  void _refreshChats() {
    // Los más recientes primero; los que no tienen mensajes, por orden de
    // creación.
    chats = p2m.chats()
      ..sort((a, b) {
        final ta = a.last?.sentAt ?? 0, tb = b.last?.sentAt ?? 0;
        return ta != tb ? tb.compareTo(ta) : b.index.compareTo(a.index);
      });
    notifyListeners();
  }

  p2m.ChatInfo? chat(int index) =>
      chats.where((c) => c.index == index).firstOrNull;

  /// Fotos de perfil por handle (`null`: sin foto). Se vacía cuando llega
  /// una nueva.
  final _avatars = <String, String?>{};

  String? avatar(String handle) =>
      _avatars.putIfAbsent(handle, () => p2m.avatarPath(handle: handle));

  Future<void> setAvatar(Uint8List? image) async {
    await p2m.setAvatar(image: image);
    _avatars.clear();
    notifyListeners();
  }

  void deleteChat(int index) {
    p2m.deleteChat(index: index);
    _refreshChats();
  }

  Future<void> acceptChat(int index) async {
    await p2m.acceptChat(index: index);
    _refreshChats();
  }

  void block(String handle) {
    p2m.block(handle: handle);
    _refreshChats();
  }

  void unblock(String handle) {
    p2m.unblock(handle: handle);
    _refreshChats();
  }

  Future<void> refreshStatus() async {
    try {
      status = await p2m.status();
      notifyListeners();
    } catch (_) {}
  }

  Future<void> createIdentity(String name) async {
    await p2m.createIdentity(name: name);
    profile = p2m.profile();
    notifyListeners();
  }

  /// Descarga un archivo recibido; el progreso queda en [downloads].
  Future<String> download(String id) async {
    downloads[id] = 0;
    notifyListeners();
    try {
      final path = await p2m.downloadFile(id: id);
      downloaded[id] = path;
      return path;
    } finally {
      downloads.remove(id);
      notifyListeners();
    }
  }

  Future<int> createChat(String? title, List<String> targets) async {
    final index = await p2m.createChat(title: title, targets: targets);
    _refreshChats();
    return index;
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    _lifecycle?.dispose();
    super.dispose();
  }
}
