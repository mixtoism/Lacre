import 'dart:async';

import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/p2m.dart' as p2m;
import '../store.dart';
import 'chat.dart';
import 'chat_actions.dart';
import 'common.dart';
import 'profile.dart';
import 'scan.dart';

class ChatListPage extends StatefulWidget {
  const ChatListPage({super.key, required this.store});

  final Store store;

  @override
  State<ChatListPage> createState() => _ChatListPageState();
}

class _ChatListPageState extends State<ChatListPage> {
  Store get store => widget.store;

  /// Búsqueda global abierta (`null`: cerrada).
  TextEditingController? _search;

  void _open(int index, {int? position, String? query}) {
    final chat = store.chat(index);
    Navigator.of(context).push(
      MaterialPageRoute(
        builder: (_) => ChatPage(
          store: store,
          index: index,
          title: chat?.name ?? context.l.chat,
          position: position,
          query: query,
        ),
      ),
    );
  }

  Future<void> _newChat() async {
    final index = await showDialog<int>(
      context: context,
      builder: (_) => NewChatDialog(store: store),
    );
    if (index != null && mounted) _open(index);
  }

  void _closeSearch() {
    _search?.dispose();
    setState(() => _search = null);
  }

  @override
  void initState() {
    super.initState();
    // Al tocar el aviso de un mensaje (Android).
    _openSub = store.openChat.stream.listen((index) {
      if (!mounted) return;
      Navigator.of(context).popUntil((r) => r.isFirst);
      _open(index);
    });
    // La primera vez, qué deben decir los avisos (D-035).
    if (store.noticeContent == null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) chooseNoticeContent(context, store);
      });
    }
  }

  late final StreamSubscription<int> _openSub;

  @override
  void dispose() {
    _openSub.cancel();
    _search?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final handle = store.profile!.handle!;
    final search = _search;
    return PopScope(
      canPop: search == null,
      onPopInvokedWithResult: (popped, _) {
        if (!popped) _closeSearch();
      },
      child: Scaffold(
        appBar: search != null
            ? AppBar(
                leading: IconButton(
                  tooltip: context.l.closeSearch,
                  icon: const Icon(Icons.arrow_back),
                  onPressed: _closeSearch,
                ),
                title: TextField(
                  controller: search,
                  autofocus: true,
                  decoration: InputDecoration(
                    hintText: context.l.searchAllChats,
                    border: InputBorder.none,
                  ),
                  onChanged: (_) => setState(() {}),
                ),
                actions: [
                  if (search.text.isNotEmpty)
                    IconButton(
                      tooltip: context.l.clear,
                      icon: const Icon(Icons.close),
                      onPressed: () => setState(search.clear),
                    ),
                ],
              )
            : AppBar(
                titleSpacing: 12,
                title: InkWell(
                  borderRadius: BorderRadius.circular(24),
                  onTap: () => Navigator.of(context).push(
                    MaterialPageRoute(
                      builder: (_) => ProfilePage(store: store),
                    ),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Avatar(
                          name: handle,
                          path: store.avatar(handle),
                          radius: 18,
                        ),
                        const SizedBox(width: 12),
                        Flexible(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                handle.split('#').first,
                                overflow: TextOverflow.ellipsis,
                              ),
                              Text(
                                '#${handle.split('#').last}',
                                style: Theme.of(context).textTheme.labelSmall
                                    ?.copyWith(
                                      color: Theme.of(context)
                                          .colorScheme
                                          .onSurfaceVariant,
                                    ),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
                actions: [
                  IconButton(
                    tooltip: context.l.search,
                    icon: const Icon(Icons.search),
                    onPressed: () =>
                        setState(() => _search = TextEditingController()),
                  ),
                  _StatusChip(store: store),
                  const SizedBox(width: 8),
                ],
              ),
        body: search != null && search.text.trim().isNotEmpty
            ? _SearchResults(store: store, query: search.text, onOpen: _open)
            : _chatList(context),
        floatingActionButton: search != null
            ? null
            : FloatingActionButton.extended(
                tooltip: context.l.newChat,
                onPressed: _newChat,
                icon: const Icon(Icons.add_comment_outlined),
                label: Text(context.l.newChat),
              ),
      ),
    );
  }

  Widget _chatList(BuildContext context) {
    final requests = store.chats.where((c) => c.request).toList();
    final chats = store.chats.where((c) => !c.request).toList();
    if (requests.isEmpty && chats.isEmpty) {
      final theme = Theme.of(context);
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(32),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(
                Icons.forum_outlined,
                size: 64,
                color: theme.colorScheme.primary,
              ),
              const SizedBox(height: 16),
              Text(context.l.noChatsYet, style: theme.textTheme.titleMedium),
              const SizedBox(height: 8),
              Text(context.l.noChatsHint, textAlign: TextAlign.center),
            ],
          ),
        ),
      );
    }
    return ListView(
      padding: const EdgeInsets.only(bottom: 88),
      children: [
        if (requests.isNotEmpty) ...[
          _Header(context.l.chatRequests, subtitle: context.l.chatRequestsHint),
          for (final c in requests)
            _ChatTile(store: store, chat: c, onOpen: _open),
          if (chats.isNotEmpty) _Header(context.l.chats),
        ],
        for (final c in chats) _ChatTile(store: store, chat: c, onOpen: _open),
      ],
    );
  }
}

class _Header extends StatelessWidget {
  const _Header(this.title, {this.subtitle});

  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            title,
            style: theme.textTheme.titleSmall?.copyWith(
              color: theme.colorScheme.primary,
            ),
          ),
          if (subtitle != null)
            Text(
              subtitle!,
              style: theme.textTheme.bodySmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
        ],
      ),
    );
  }
}

/// Resumen del último mensaje para la lista.
String preview(AppLocalizations l, p2m.Message m) {
  if (m.notice) return m.text;
  final body = m.sticker != null
      ? l.sticker
      : m.file != null
      ? '📎 ${m.file!.name}'
      : m.text;
  return '${m.mine ? l.you : m.from.split('#').first}: $body';
}

class _ChatTile extends StatelessWidget {
  const _ChatTile({
    required this.store,
    required this.chat,
    required this.onOpen,
  });

  final Store store;
  final p2m.ChatInfo chat;
  final void Function(int index) onOpen;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final c = chat;
    final last = c.last;
    return GestureDetector(
      onSecondaryTap: () => chatMenu(context, store, c),
      child: ListTile(
        contentPadding: const EdgeInsets.symmetric(horizontal: 16, vertical: 2),
        leading: Opacity(
          opacity: c.active ? 1 : 0.5,
          child: ChatAvatar(chat: c),
        ),
        title: Row(
          children: [
            Flexible(
              child: Text(
                c.name,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: TextStyle(
                  fontWeight: FontWeight.w600,
                  color: c.active ? null : theme.disabledColor,
                ),
              ),
            ),
            if (c.request) ...[
              const SizedBox(width: 8),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                decoration: BoxDecoration(
                  color: theme.colorScheme.tertiaryContainer,
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Text(
                  context.l.request,
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.onTertiaryContainer,
                  ),
                ),
              ),
            ],
          ],
        ),
        subtitle: Text(
          !c.active
              ? context.l.notInChat
              : last == null
              ? (c.peer == null
                    ? context.l.groupNoMessages
                    : context.l.noMessages)
              : preview(context.l, last),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
        ),
        trailing: last == null
            ? null
            : Text(hhmm(last.sentAt), style: theme.textTheme.labelSmall),
        onTap: () => onOpen(c.index),
        onLongPress: () => chatMenu(context, store, c),
      ),
    );
  }
}

/// Búsqueda global: chats por nombre y mensajes de todos los chats.
class _SearchResults extends StatelessWidget {
  const _SearchResults({
    required this.store,
    required this.query,
    required this.onOpen,
  });

  final Store store;
  final String query;
  final void Function(int index, {int? position, String? query}) onOpen;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final q = fold(query.trim());
    final chats = store.chats
        .where(
          (c) => fold(c.name).contains(q) || fold(c.peer ?? '').contains(q),
        )
        .toList();
    final hits = p2m.search(query: query);
    if (chats.isEmpty && hits.isEmpty) {
      return Center(child: Text(context.l.noResults));
    }
    return ListView(
      children: [
        if (chats.isNotEmpty) _Header(context.l.chats),
        for (final c in chats)
          ListTile(
            leading: ChatAvatar(chat: c),
            title: Highlighted(text: c.name, query: query, maxLines: 1),
            onTap: () => onOpen(c.index),
          ),
        if (hits.isNotEmpty) _Header(context.l.messageCount(hits.length)),
        for (final h in hits)
          Builder(
            builder: (context) {
              final chat = store.chat(h.index);
              final m = h.message;
              return ListTile(
                leading: chat == null
                    ? const Avatar(name: '?')
                    : ChatAvatar(chat: chat),
                title: Text(
                  chat?.name ?? context.l.chat,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                subtitle: Highlighted(
                  text: preview(context.l, m),
                  query: query,
                  maxLines: 2,
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                trailing: Text(
                  hhmm(m.sentAt),
                  style: theme.textTheme.labelSmall,
                ),
                onTap: () =>
                    onOpen(h.index, position: h.position, query: query),
              );
            },
          ),
      ],
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
        context.l.peersConnected(peers),
        if (store.status?.onion != null) context.l.onionPublished,
        if (store.relay != null) context.l.reachableByRelay,
      ].join('\n'),
      child: Chip(
        avatar: Icon(
          Icons.circle,
          size: 12,
          color: peers > 0 ? scheme.primary : scheme.error,
        ),
        label: Text(peers > 0 ? '$peers' : context.l.offline),
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

  /// Añade a los participantes el código escaneado.
  Future<void> _scan() async {
    final code = await scanQr(
      context,
      title: context.l.scanProfileQr,
      accept: (v) => v.startsWith('p2m:'),
    );
    if (code == null || !mounted) return;
    final current = _targets.text.trim();
    setState(() => _targets.text = current.isEmpty ? code : '$current\n$code');
  }

  Future<void> _create() async {
    final targets = _targets.text
        .split(RegExp(r'[\s,]+'))
        .where((t) => t.isNotEmpty)
        .toList();
    if (targets.isEmpty) return;
    setState(() => _busy = true);
    final title = _title.text.trim();
    final index = await guard(
      context,
      () => widget.store.createChat(title.isEmpty ? null : title, targets),
    );
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
      title: Text(context.l.newChat),
      content: SizedBox(
        width: 420,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                controller: _targets,
                autofocus: true,
                minLines: 2,
                maxLines: 5,
                enabled: !_busy,
                decoration: InputDecoration(
                  labelText: context.l.participants,
                  helperText: context.l.participantsHelper,
                  border: const OutlineInputBorder(),
                  suffixIcon: canScan
                      ? IconButton(
                          tooltip: context.l.scanQr,
                          icon: const Icon(Icons.qr_code_scanner),
                          onPressed: _busy ? null : _scan,
                        )
                      : null,
                ),
              ),
              const SizedBox(height: 16),
              TextField(
                controller: _title,
                enabled: !_busy,
                decoration: InputDecoration(
                  labelText: context.l.groupTitleOptional,
                  border: const OutlineInputBorder(),
                ),
              ),
              if (_busy) ...[
                const SizedBox(height: 16),
                const LinearProgressIndicator(),
                const SizedBox(height: 8),
                Text(context.l.searchingNetwork),
              ],
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(),
          child: Text(context.l.cancel),
        ),
        FilledButton(
          onPressed: _busy ? null : _create,
          child: Text(context.l.create),
        ),
      ],
    );
  }
}
