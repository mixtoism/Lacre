import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:path_provider/path_provider.dart';

import 'src/rust/api/p2m.dart' as p2m;

/// Estado de la app: perfil, chats y avisos del nodo.
class Store extends ChangeNotifier {
  p2m.Profile? profile;
  List<p2m.ChatInfo> chats = [];
  p2m.Status? status;
  String? relay;

  /// Avisos para mostrar como SnackBar.
  final notices = StreamController<String>.broadcast();

  /// Mensajes nuevos, para la pantalla del chat abierto.
  final messages = StreamController<(int, p2m.Message)>.broadcast();

  Timer? _statusTimer;

  Future<void> start({String? dataDir}) async {
    // P2M_DATA permite arrancar varias instancias en el mismo equipo.
    final dir = dataDir ??
        Platform.environment['P2M_DATA'] ??
        '${(await getApplicationSupportDirectory()).path}/node';
    profile = await p2m.start(dataDir: dir);
    p2m.eventStream().listen(_onEvent);
    _refreshChats();
    _statusTimer = Timer.periodic(const Duration(seconds: 5), (_) => refreshStatus());
    refreshStatus();
    notifyListeners();
  }

  void _onEvent(p2m.UiEvent ev) {
    switch (ev) {
      case p2m.UiEvent_Linked(:final handle):
        notices.add('Dispositivo vinculado a $handle');
        profile = p2m.profile();
      case p2m.UiEvent_NewChat(:final name):
        notices.add('Nuevo chat: $name');
      case p2m.UiEvent_Message(:final index, :final message):
        messages.add((index, message));
      case p2m.UiEvent_Removed():
        notices.add('Este dispositivo ha sido expulsado de un chat');
      case p2m.UiEvent_UnverifiedMember(:final name):
        notices.add('⚠ Miembro no verificado en el chat: $name');
      case p2m.UiEvent_RelayReady(:final addr):
        relay = addr;
    }
    _refreshChats();
  }

  void _refreshChats() {
    chats = p2m.chats();
    notifyListeners();
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

  Future<int> createChat(String? title, List<String> targets) async {
    final index = await p2m.createChat(title: title, targets: targets);
    _refreshChats();
    return index;
  }

  @override
  void dispose() {
    _statusTimer?.cancel();
    super.dispose();
  }
}
