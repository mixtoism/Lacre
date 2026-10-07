import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/lacre.dart' as lacre;
import '../store.dart';
import 'common.dart';

/// Acciones sobre un chat: aceptar la solicitud, bloquear y borrar (D-029).
/// Las usan la lista de chats y la pantalla del chat.

/// Borra el chat tras confirmarlo. Devuelve si se borró.
Future<bool> deleteChat(
  BuildContext context,
  Store store,
  lacre.ChatInfo chat,
) async {
  // En un grupo que no gestionamos, borrar es salir de él (D-031).
  var manager = false;
  if (chat.peer == null && chat.active) {
    try {
      manager = lacre.chatMembers(index: chat.index).canEdit;
    } catch (_) {}
  }
  final l = context.l;
  final String body;
  if (chat.peer != null || !chat.active) {
    body = l.deleteChatBodyDirect;
  } else if (manager) {
    body = l.deleteChatBodyManager;
  } else {
    body = l.deleteChatBodyLeave;
  }
  final ok = await confirm(
    context,
    title: l.deleteChatTitle(chat.name),
    body: body,
    action: manager || chat.peer != null || !chat.active
        ? l.delete
        : l.leaveAndDelete,
  );
  if (!ok || !context.mounted) return false;
  final done = await guard(context, () async {
    store.deleteChat(chat.index);
    return true;
  });
  return done == true;
}

/// Bloquea a una persona tras confirmarlo. Devuelve si se bloqueó.
Future<bool> blockPerson(
  BuildContext context,
  Store store,
  String handle,
) async {
  final ok = await confirm(
    context,
    title: context.l.blockTitle(handle.split('#').first),
    body: context.l.blockBody,
    action: context.l.block,
  );
  if (!ok || !context.mounted) return false;
  final done = await guard(context, () async {
    store.block(handle);
    return true;
  });
  return done == true;
}

Future<void> acceptChat(BuildContext context, Store store, int index) =>
    guard(context, () => store.acceptChat(index));

/// Menú de un chat (pulsación larga o clic derecho en la lista).
Future<void> chatMenu(
  BuildContext context,
  Store store,
  lacre.ChatInfo chat,
) async {
  final peer = chat.peer;
  final action = await showModalBottomSheet<String>(
    context: context,
    showDragHandle: true,
    builder: (context) => SafeArea(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (chat.request)
            ListTile(
              leading: const Icon(Icons.check_circle_outline),
              title: Text(context.l.acceptRequest),
              onTap: () => Navigator.pop(context, 'accept'),
            ),
          if (peer != null)
            ListTile(
              leading: const Icon(Icons.block),
              title: Text(context.l.blockPerson(peer.split('#').first)),
              onTap: () => Navigator.pop(context, 'block'),
            ),
          ListTile(
            leading: Icon(
              Icons.delete_outline,
              color: Theme.of(context).colorScheme.error,
            ),
            title: Text(context.l.deleteChat),
            onTap: () => Navigator.pop(context, 'delete'),
          ),
        ],
      ),
    ),
  );
  if (!context.mounted) return;
  switch (action) {
    case 'accept':
      await acceptChat(context, store, chat.index);
    case 'block':
      await blockPerson(context, store, peer!);
    case 'delete':
      await deleteChat(context, store, chat);
  }
}

/// Foto del chat: la de la otra persona en un 1:1, un icono en un grupo.
class ChatAvatar extends StatelessWidget {
  const ChatAvatar({super.key, required this.chat, this.radius = 24});

  final lacre.ChatInfo chat;
  final double radius;

  @override
  Widget build(BuildContext context) {
    final peer = chat.peer;
    return Avatar(
      name: peer ?? chat.name,
      path: chat.avatar,
      radius: radius,
      group: peer == null,
    );
  }
}
