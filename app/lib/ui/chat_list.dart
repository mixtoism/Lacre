import 'package:flutter/material.dart';

import '../store.dart';
import 'chat.dart';
import 'common.dart';
import 'profile.dart';

class ChatListPage extends StatelessWidget {
  const ChatListPage({super.key, required this.store});

  final Store store;

  Future<void> _newChat(BuildContext context) async {
    final index = await showDialog<int>(context: context, builder: (_) => NewChatDialog(store: store));
    if (index != null && context.mounted) {
      final chat = store.chats.where((c) => c.index == index).firstOrNull;
      Navigator.of(context).push(MaterialPageRoute(
        builder: (_) => ChatPage(store: store, index: index, title: chat?.name ?? 'Chat'),
      ));
    }
  }

  @override
  Widget build(BuildContext context) {
    final chats = store.chats.reversed.toList();
    return Scaffold(
      appBar: AppBar(
        title: Text(store.profile!.handle!),
        actions: [
          _StatusChip(store: store),
          IconButton(
            tooltip: 'Perfil',
            icon: const Icon(Icons.account_circle_outlined),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(builder: (_) => ProfilePage(store: store)),
            ),
          ),
          const SizedBox(width: 8),
        ],
      ),
      body: chats.isEmpty
          ? const Center(child: Text('Aún no tienes chats.\nPulsa + para empezar uno.', textAlign: TextAlign.center))
          : ListView.builder(
              itemCount: chats.length,
              itemBuilder: (context, i) {
                final c = chats[i];
                final last = c.last;
                return ListTile(
                  enabled: c.active,
                  leading: CircleAvatar(child: Text(c.name.isEmpty ? '?' : c.name[0].toUpperCase())),
                  title: Text(c.name, maxLines: 1, overflow: TextOverflow.ellipsis),
                  subtitle: Text(
                    !c.active
                        ? 'Ya no participas en este chat'
                        : last == null
                            ? '${c.devices} dispositivos'
                            : '${last.mine ? 'Tú' : last.from.split('#').first}: ${last.text}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  trailing: last == null ? null : Text(hhmm(last.sentAt)),
                  onTap: () => Navigator.of(context).push(MaterialPageRoute(
                    builder: (_) => ChatPage(store: store, index: c.index, title: c.name),
                  )),
                );
              },
            ),
      floatingActionButton: FloatingActionButton(
        tooltip: 'Nuevo chat',
        onPressed: () => _newChat(context),
        child: const Icon(Icons.add_comment_outlined),
      ),
    );
  }
}

class _StatusChip extends StatelessWidget {
  const _StatusChip({required this.store});

  final Store store;

  @override
  Widget build(BuildContext context) {
    final peers = store.status?.peers ?? 0;
    final scheme = Theme.of(context).colorScheme;
    return Tooltip(
      message: [
        '$peers peers conectados',
        if (store.status?.onion != null) 'Servicio onion publicado',
        if (store.relay != null) 'Alcanzable por relay',
      ].join('\n'),
      child: Chip(
        avatar: Icon(Icons.circle, size: 12, color: peers > 0 ? Colors.green : scheme.error),
        label: Text(peers > 0 ? '$peers' : 'sin red'),
        visualDensity: VisualDensity.compact,
      ),
    );
  }
}

class NewChatDialog extends StatefulWidget {
  const NewChatDialog({super.key, required this.store});

  final Store store;

  @override
  State<NewChatDialog> createState() => _NewChatDialogState();
}

class _NewChatDialogState extends State<NewChatDialog> {
  final _title = TextEditingController();
  final _targets = TextEditingController();
  bool _busy = false;

  Future<void> _create() async {
    final targets = _targets.text.split(RegExp(r'[\s,]+')).where((t) => t.isNotEmpty).toList();
    if (targets.isEmpty) return;
    setState(() => _busy = true);
    final title = _title.text.trim();
    final index = await guard(context, () => widget.store.createChat(title.isEmpty ? null : title, targets));
    if (!mounted) return;
    if (index != null) {
      Navigator.of(context).pop(index);
    } else {
      setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: const Text('Nuevo chat'),
      content: SizedBox(
        width: 420,
        child: Column(mainAxisSize: MainAxisSize.min, children: [
          TextField(
            controller: _targets,
            autofocus: true,
            minLines: 2,
            maxLines: 5,
            enabled: !_busy,
            decoration: const InputDecoration(
              labelText: 'Participantes',
              helperText: 'Handles (nombre#sufijo) o códigos QR (p2m:…),\nseparados por espacios o líneas',
              border: OutlineInputBorder(),
            ),
          ),
          const SizedBox(height: 16),
          TextField(
            controller: _title,
            enabled: !_busy,
            decoration: const InputDecoration(
              labelText: 'Título del grupo (opcional)',
              border: OutlineInputBorder(),
            ),
          ),
          if (_busy) ...[
            const SizedBox(height: 16),
            const LinearProgressIndicator(),
            const SizedBox(height: 8),
            const Text('Buscando en la red… (por Tor puede tardar un minuto)'),
          ],
        ]),
      ),
      actions: [
        TextButton(onPressed: _busy ? null : () => Navigator.of(context).pop(), child: const Text('Cancelar')),
        FilledButton(onPressed: _busy ? null : _create, child: const Text('Crear')),
      ],
    );
  }
}
