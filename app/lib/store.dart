import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_local_notifications_linux/flutter_local_notifications_linux.dart';
import 'package:path_provider/path_provider.dart';

import 'l10n/l10n.dart';
import 'src/rust/api/lacre.dart' as lacre;
import 'ui/common.dart' show avatarOf, forgetAvatars;

/// Canal con el código nativo de Android (LacreApp.kt): clave de datos,
/// servicio en segundo plano, avisos y abrir archivos.
const sistema = MethodChannel('lacre/sistema');

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
  lacre.Profile? profile;
  List<lacre.ChatInfo> chats = [];
  lacre.Status? status;
  String? relay;

  /// Error al arrancar el nodo.
  String? startError;

  /// Avisos para mostrar como SnackBar.
  final notices = StreamController<String>.broadcast();

  /// Mensajes nuevos, para la pantalla del chat abierto.
  final messages = StreamController<(int, lacre.Message)>.broadcast();

  /// Descargas en curso: id del archivo → fracción descargada.
  final downloads = <String, double>{};

  /// Archivos descargados en esta sesión.
  final downloaded = <String>{};

  Timer? _statusTimer;

  /// Modo de energía (D-034): en Android, reposo con la app en segundo
  /// plano; mDNS solo con wifi.
  AppLifecycleListener? _lifecycle;
  bool _background = false;
  bool _wifi = true;

  /// Android: minutos entre comprobaciones de mensajes con la app cerrada;
  /// 0, siempre conectado a Tor (D-038). `null` en el escritorio.
  int? checkEvery;

  /// Intervalos que se ofrecen. JobScheduler no baja de 15 min.
  static const checkIntervals = [15, 30, 60, 120];

  /// Chats que hay que abrir (al tocar el aviso de un mensaje).
  final openChat = StreamController<int>.broadcast();

  /// Un dispositivo nuevo ha entrado con nuestra invitación (D-037).
  final deviceLinked = StreamController<String>.broadcast();

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

  /// Con `lanOnly` (o `LACRE_LAN=1`), sin Tor: solo red local, para pruebas.
  Future<void> start({String? dataDir, bool lanOnly = false}) async {
    // LACRE_DATA permite arrancar varias instancias en el mismo equipo.
    final dir =
        dataDir ??
        Platform.environment['LACRE_DATA'] ??
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
      profile = await lacre.start(
        dataDir: dir,
        lanOnly: lanOnly || Platform.environment['LACRE_LAN'] == '1',
        dataKey: key,
      );
    } catch (e) {
      debugPrint('lacre arranque: $e');
      startError = '$e';
      notifyListeners();
      return;
    }
    lacre.eventStream().listen(_onEvent);
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
      checkEvery = await sistema.invokeMethod<int>('intervalo') ?? 0;
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
            .then((r) => debugPrint('lacre servicio: $r'))
            .catchError((Object e) => debugPrint('lacre servicio: $e')),
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
        debugPrint('lacre avisos: $e');
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
    // Con intervalo (D-038) el nodo solo corre en segundo plano durante una
    // comprobación, que conviene que sea rápida: sin reposo.
    final low = _background && checkEvery == 0;
    debugPrint(
      'lacre energía: segundo plano=$_background reposo=$low wifi=$_wifi',
    );
    try {
      lacre.setPower(background: low, wifi: _wifi);
    } catch (e) {
      debugPrint('lacre energía: $e');
    }
    // El lock de multicast impide que el wifi filtre paquetes: solo
    // mientras mDNS esté activo.
    unawaited(
      sistema
          .invokeMethod('multicast', !low && _wifi)
          .catchError((Object e) => debugPrint('lacre multicast: $e')),
    );
    if (_background) {
      // Nadie mira el estado: no despertar al nodo cada 5 s.
      _statusTimer?.cancel();
      _statusTimer = null;
    } else if (_statusTimer == null) {
      _startStatusTimer();
    }
  }

  void _onEvent(lacre.UiEvent ev) {
    switch (ev) {
      case lacre.UiEvent_FileProgress(:final id, :final done, :final total):
        if (downloads.containsKey(id)) {
          downloads[id] = total == 0 ? 1 : done / total;
          notifyListeners();
        }
        return;
      case lacre.UiEvent_Linked(:final handle):
        notices.add(systemL10n.deviceLinked(handle));
        profile = lacre.profile();
      case lacre.UiEvent_DeviceLinked(:final label):
        notices.add(systemL10n.deviceJoined(label));
        deviceLinked.add(label);
      case lacre.UiEvent_NewChat(:final index, :final name, :final request):
        final text = request
            ? systemL10n.chatRequestFrom(name)
            : systemL10n.newChatWith(name);
        notices.add(text);
        _notify(index, 'Lacre', text);
      case lacre.UiEvent_Avatar():
        forgetAvatars();
      case lacre.UiEvent_ChatsChanged():
        break;
      case lacre.UiEvent_Message(:final index, :final message, :final local):
        messages.add((index, message));
        if (!local && !message.mine && !message.notice) {
          _notifyMessage(index, message);
        }
      case lacre.UiEvent_Removed():
        notices.add(systemL10n.expelled);
      case lacre.UiEvent_UnverifiedMember(:final name):
        notices.add(systemL10n.unverifiedMember(name));
      case lacre.UiEvent_RelayReady(:final addr):
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
          .catchError((Object e) => debugPrint('lacre aviso: $e'));
    }
  }

  Future<void> setNoticeContent(NoticeContent c) async {
    noticeContent = c;
    notifyListeners();
    try {
      await _noticeFile?.writeAsString(c.name);
    } catch (e) {
      debugPrint('lacre avisos: $e');
    }
  }

  /// Android: siempre conectado (0) o comprobar cada `minutes` (D-038).
  Future<void> setCheckEvery(int minutes) async {
    checkEvery = minutes;
    notifyListeners();
    _applyPower();
    try {
      await sistema.invokeMethod('fijarIntervalo', minutes);
    } catch (e) {
      debugPrint('lacre intervalo: $e');
    }
  }

  void _notifyMessage(int index, lacre.Message m) {
    final chat = lacre.chats().where((c) => c.index == index).firstOrNull;
    if (chat == null) return;
    final content = noticeContent ?? NoticeContent.none;
    if (content == NoticeContent.none) {
      _notify(_generalNotice, 'Lacre', systemL10n.youHaveNewMessages);
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
      _notify(index, 'Lacre', systemL10n.chatRequestFrom(chat.name));
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
    chats = lacre.chats()
      ..sort((a, b) {
        final ta = a.last?.sentAt ?? 0, tb = b.last?.sentAt ?? 0;
        return ta != tb ? tb.compareTo(ta) : b.index.compareTo(a.index);
      });
    notifyListeners();
  }

  lacre.ChatInfo? chat(int index) =>
      chats.where((c) => c.index == index).firstOrNull;

  /// Foto de perfil de un handle (`null`: sin foto), descifrada.
  Uint8List? avatar(String handle) => avatarOf(handle);

  Future<void> setAvatar(Uint8List? image) async {
    await lacre.setAvatar(image: image);
    forgetAvatars();
    notifyListeners();
  }

  void deleteChat(int index) {
    lacre.deleteChat(index: index);
    _refreshChats();
  }

  Future<void> acceptChat(int index) async {
    await lacre.acceptChat(index: index);
    _refreshChats();
  }

  void block(String handle) {
    lacre.block(handle: handle);
    _refreshChats();
  }

  void unblock(String handle) {
    lacre.unblock(handle: handle);
    _refreshChats();
  }

  Future<void> refreshStatus() async {
    try {
      status = await lacre.status();
      notifyListeners();
    } catch (_) {}
  }

  Future<void> createIdentity(String name) async {
    await lacre.createIdentity(name: name);
    profile = lacre.profile();
    notifyListeners();
  }

  /// Descarga un archivo recibido; el progreso queda en [downloads].
  Future<void> download(String id) async {
    downloads[id] = 0;
    notifyListeners();
    try {
      await lacre.downloadFile(id: id);
      downloaded.add(id);
    } finally {
      downloads.remove(id);
      notifyListeners();
    }
  }

  Future<int> createChat(String? title, List<String> targets) async {
    final index = await lacre.createChat(title: title, targets: targets);
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
