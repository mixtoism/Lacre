import 'dart:async';
import 'dart:io';

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:scrollable_positioned_list/scrollable_positioned_list.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/lacre.dart' as lacre;
import '../store.dart';
import 'chat_actions.dart';
import 'common.dart';
import 'members.dart';
import 'stickers.dart';

class ChatPage extends StatefulWidget {
  const ChatPage({
    super.key,
    required this.store,
    required this.index,
    required this.title,
    this.position,
    this.query,
  });

  final Store store;
  final int index;
  final String title;

  /// Mensaje al que saltar al abrir (de la búsqueda global) y texto buscado.
  final int? position;
  final String? query;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  late List<lacre.Message> _messages;
  late StreamSubscription<(int, lacre.Message)> _sub;
  late StreamSubscription<int> _history;

  /// Panel de emojis y stickers abierto.
  bool _panel = false;

  /// Archivos que se están cifrando para enviar.
  int _preparing = 0;

  final _scroll = ItemScrollController();

  /// Búsqueda dentro del chat (`null`: cerrada).
  TextEditingController? _search;

  /// Resultados (posiciones en `_messages`, del más reciente al más
  /// antiguo) y el que está seleccionado.
  List<int> _hits = [];
  int _hit = 0;

  /// Mensaje resaltado (el de la búsqueda).
  int? _marked;

  @override
  void initState() {
    super.initState();
    widget.store.view(widget.index);
    _messages = lacre.messages(index: widget.index);
    final position = widget.position;
    if (position != null) {
      _marked = position;
      if (widget.query != null) {
        _search = TextEditingController(text: widget.query);
        _hits = _find(widget.query!);
        _hit = _hits.indexOf(position).clamp(0, _hits.length);
      }
      WidgetsBinding.instance.addPostFrameCallback((_) => _jump(position));
    }
    _sub = widget.store.messages.stream
        .where((m) => m.$1 == widget.index)
        .listen((m) {
          setState(() => _messages.add(m.$2));
        });
    _history = widget.store.historyLoaded.stream
        .where((i) => i == widget.index)
        .listen((_) {
          setState(() => _messages = lacre.messages(index: widget.index));
        });
  }

  List<int> _find(String query) => query.trim().isEmpty
      ? []
      : lacre
            .search(query: query, index: widget.index)
            .map((h) => h.position)
            .toList();

  /// Lleva la lista al mensaje `position` (la lista va invertida).
  void _jump(int position) {
    if (!_scroll.isAttached || position >= _messages.length) return;
    setState(() => _marked = position);
    _scroll.scrollTo(
      index: _messages.length - 1 - position,
      alignment: 0.4,
      duration: const Duration(milliseconds: 250),
      curve: Curves.easeOut,
    );
  }

  void _openSearch() => setState(() {
    _search = TextEditingController();
    _hits = [];
  });

  void _closeSearch() {
    _search?.dispose();
    setState(() {
      _search = null;
      _hits = [];
      _marked = null;
    });
  }

  void _onQuery(String query) {
    setState(() {
      _hits = _find(query);
      _hit = 0;
    });
    if (_hits.isNotEmpty) _jump(_hits.first);
  }

  /// `older`: hacia mensajes más antiguos.
  void _step(bool older) {
    if (_hits.isEmpty) return;
    final next = (_hit + (older ? 1 : -1)) % _hits.length;
    setState(() => _hit = next);
    _jump(_hits[next]);
  }

  @override
  void dispose() {
    if (widget.store.viewing == widget.index) widget.store.view(null);
    _search?.dispose();
    _sub.cancel();
    _history.cancel();
    _input.dispose();
    _focus.dispose();
    super.dispose();
  }

  /// No se espera a la entrega: con dispositivos desconectados puede tardar
  /// minutos. El mensaje aparece en cuanto el núcleo lo guarda.
  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty) return;
    _input.clear();
    if (!_panel) _focus.requestFocus();
    await _deliver(() => lacre.send(index: widget.index, text: text));
  }

  Future<void> _sendSticker(lacre.Sticker s) =>
      _deliver(() => lacre.sendSticker(index: widget.index, id: s.id));

  Future<void> _sendFile() async {
    final messenger = ScaffoldMessenger.of(context);
    final l = context.l;
    final picked = await openFile();
    if (picked == null) return;
    final size = await picked.length();
    final max = lacre.maxFileBytes().toInt();
    if (size > max) {
      messenger.showSnackBar(
        SnackBar(content: Text(l.fileTooBig(fileSize(size), fileSize(max)))),
      );
      return;
    }
    setState(() => _preparing++);
    try {
      await _deliver(
        () => lacre.sendFile(index: widget.index, path: picked.path),
      );
    } finally {
      if (mounted) setState(() => _preparing--);
      await _dropPickerCopy(picked.path);
    }
  }

  /// En Android el selector copia el archivo a la caché de la app; el núcleo
  /// ya guardó el suyo, así que esa copia sobra.
  Future<void> _dropPickerCopy(String path) async {
    if (!Platform.isAndroid) return;
    final cache = (await getTemporaryDirectory()).path;
    final dir = File(path).parent;
    if (dir.parent.path == cache) {
      try {
        await dir.delete(recursive: true);
      } catch (_) {}
    }
  }

  void _togglePanel() {
    setState(() => _panel = !_panel);
    // En el móvil el teclado del sistema y el panel no caben a la vez.
    if (_panel && (Platform.isAndroid || Platform.isIOS)) {
      _focus.unfocus();
    } else {
      _focus.requestFocus();
    }
  }

  Future<void> _deliver(Future<lacre.SendResult> Function() send) async {
    final messenger = ScaffoldMessenger.of(context);
    final l = context.l;
    final error = Theme.of(context).colorScheme.error;
    try {
      final r = await send();
      if (r.failed > 0) {
        messenger.showSnackBar(
          SnackBar(content: Text(l.deliveryFailed(r.failed))),
        );
      }
    } catch (e) {
      debugPrint('send failed: $e');
      messenger.showSnackBar(
        SnackBar(content: Text('$e'), backgroundColor: error),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    // Escucha al almacén: el chat puede cambiar de nombre (personas que
    // entran o salen) o dejar de estar activo (nos han quitado).
    return ListenableBuilder(
      listenable: widget.store,
      builder: (context, _) => _build(context),
    );
  }

  Widget _build(BuildContext context) {
    final chat = widget.store.chat(widget.index);
    final active = chat?.active ?? true;
    final request = chat?.request ?? false;
    final group = chat != null ? chat.peer == null : false;
    final query = _search?.text ?? '';
    return PopScope(
      canPop: _search == null,
      onPopInvokedWithResult: (popped, _) {
        if (!popped) _closeSearch();
      },
      child: Scaffold(
        appBar: _search != null ? _searchBar() : _appBar(context, chat),
        body: Column(
          children: [
            Expanded(
              child: ScrollablePositionedList.builder(
                itemScrollController: _scroll,
                reverse: true,
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 8,
                ),
                itemCount: _messages.length,
                itemBuilder: (context, i) {
                  final position = _messages.length - 1 - i;
                  final m = _messages[position];
                  // Mensajes seguidos de la misma persona: el nombre y la
                  // foto, solo en el primero.
                  final prev = position > 0 ? _messages[position - 1] : null;
                  final first =
                      prev == null ||
                      prev.notice ||
                      prev.from != m.from ||
                      m.sentAt - prev.sentAt > 300;
                  final Widget child;
                  if (m.notice) {
                    child = _Notice(message: m);
                  } else {
                    final body = m.file != null
                        ? _FileBubble(
                            store: widget.store,
                            message: m,
                            locked: request,
                          )
                        : m.sticker != null
                        ? _StickerBubble(message: m, hidden: request)
                        : _Bubble(message: m, query: query);
                    child = _Line(
                      store: widget.store,
                      message: m,
                      showSender: (group || !m.mine) && first && !m.mine,
                      avatar: group && !m.mine,
                      first: first,
                      child: body,
                    );
                  }
                  return AnimatedContainer(
                    key: ValueKey(position),
                    duration: const Duration(milliseconds: 300),
                    decoration: BoxDecoration(
                      color: _marked == position
                          ? Theme.of(context).colorScheme.tertiaryContainer
                                .withValues(alpha: 0.5)
                          : null,
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: child,
                  );
                },
              ),
            ),
            if (_preparing > 0) const LinearProgressIndicator(),
            if (request && chat != null)
              _RequestBanner(store: widget.store, chat: chat),
            const Divider(height: 1),
            SafeArea(
              child: Padding(
                padding: const EdgeInsets.all(8),
                child: active
                    ? _composer()
                    : Padding(
                        padding: const EdgeInsets.all(8),
                        child: Text(context.l.notInChatDot),
                      ),
              ),
            ),
            if (active && _panel)
              ExpressionPanel(input: _input, onSticker: _sendSticker),
          ],
        ),
      ),
    );
  }

  PreferredSizeWidget _appBar(BuildContext context, lacre.ChatInfo? chat) {
    final peer = chat?.peer;
    return AppBar(
      titleSpacing: 0,
      title: Row(
        children: [
          if (chat != null) ...[
            ChatAvatar(chat: chat, radius: 18),
            const SizedBox(width: 12),
          ],
          Flexible(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  peer?.split('#').first ?? chat?.name ?? widget.title,
                  overflow: TextOverflow.ellipsis,
                ),
                Text(
                  peer != null
                      ? '#${peer.split('#').last}'
                      : context.l.deviceCount(chat?.devices ?? 0),
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
      actions: [
        IconButton(
          tooltip: context.l.searchInChat,
          icon: const Icon(Icons.search),
          onPressed: _openSearch,
        ),
        PopupMenuButton<String>(
          tooltip: context.l.more,
          onSelected: (v) async {
            final store = widget.store;
            switch (v) {
              case 'members':
                Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) =>
                        MembersPage(store: store, index: widget.index),
                  ),
                );
              case 'block':
                if (await blockPerson(context, store, peer!) && mounted) {
                  Navigator.of(this.context).pop();
                }
              case 'delete':
                if (chat != null &&
                    await deleteChat(context, store, chat) &&
                    mounted) {
                  Navigator.of(this.context).pop();
                }
            }
          },
          itemBuilder: (context) => [
            PopupMenuItem(
              value: 'members',
              child: ListTile(
                leading: const Icon(Icons.group_outlined),
                title: Text(context.l.members),
              ),
            ),
            if (peer != null)
              PopupMenuItem(
                value: 'block',
                child: ListTile(
                  leading: const Icon(Icons.block),
                  title: Text(context.l.blockPerson(peer.split('#').first)),
                ),
              ),
            PopupMenuItem(
              value: 'delete',
              child: ListTile(
                leading: const Icon(Icons.delete_outline),
                title: Text(context.l.deleteChat),
              ),
            ),
          ],
        ),
      ],
    );
  }

  PreferredSizeWidget _searchBar() {
    final search = _search!;
    final count = search.text.trim().isEmpty
        ? ''
        : _hits.isEmpty
        ? '0'
        : '${_hit + 1}/${_hits.length}';
    return AppBar(
      leading: IconButton(
        tooltip: context.l.closeSearch,
        icon: const Icon(Icons.arrow_back),
        onPressed: _closeSearch,
      ),
      title: TextField(
        controller: search,
        autofocus: widget.position == null,
        decoration: InputDecoration(
          hintText: context.l.searchThisChat,
          border: InputBorder.none,
        ),
        textInputAction: TextInputAction.search,
        onChanged: _onQuery,
        onSubmitted: (_) => _step(true),
      ),
      actions: [
        Center(
          child: Text(count, style: Theme.of(context).textTheme.labelLarge),
        ),
        IconButton(
          tooltip: context.l.previousOlder,
          icon: const Icon(Icons.keyboard_arrow_up),
          onPressed: _hits.length > 1 ? () => _step(true) : null,
        ),
        IconButton(
          tooltip: context.l.nextNewer,
          icon: const Icon(Icons.keyboard_arrow_down),
          onPressed: _hits.length > 1 ? () => _step(false) : null,
        ),
      ],
    );
  }

  Widget _composer() {
    return Row(
      children: [
        IconButton(
          tooltip: context.l.emojiAndStickers,
          onPressed: _togglePanel,
          icon: Icon(
            _panel ? Icons.keyboard_outlined : Icons.emoji_emotions_outlined,
          ),
        ),
        IconButton(
          tooltip: context.l.attachFile,
          onPressed: _sendFile,
          icon: const Icon(Icons.attach_file),
        ),
        Expanded(
          child: TextField(
            controller: _input,
            focusNode: _focus,
            autofocus: _search == null,
            minLines: 1,
            maxLines: 5,
            textInputAction: TextInputAction.send,
            onSubmitted: (_) => _send(),
            onTap: () {
              if (_panel && (Platform.isAndroid || Platform.isIOS)) {
                setState(() => _panel = false);
              }
            },
            decoration: InputDecoration(
              hintText: context.l.message,
              filled: true,
              fillColor: Theme.of(context).colorScheme.surfaceContainerHigh,
              border: const OutlineInputBorder(
                borderRadius: BorderRadius.all(Radius.circular(24)),
                borderSide: BorderSide.none,
              ),
              contentPadding: const EdgeInsets.symmetric(
                horizontal: 16,
                vertical: 10,
              ),
            ),
          ),
        ),
        const SizedBox(width: 8),
        IconButton.filled(
          tooltip: context.l.send,
          onPressed: _send,
          icon: const Icon(Icons.send),
        ),
      ],
    );
  }
}

/// Solicitud de chat (D-029): aceptar, bloquear o borrar. Responder también
/// la acepta.
class _RequestBanner extends StatelessWidget {
  const _RequestBanner({required this.store, required this.chat});

  final Store store;
  final lacre.ChatInfo chat;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final peer = chat.peer;
    return Container(
      width: double.infinity,
      color: theme.colorScheme.tertiaryContainer,
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            peer != null
                ? context.l.wantsToChat(peer.split('#').first)
                : context.l.invitedTo(chat.name),
            style: theme.textTheme.titleSmall?.copyWith(
              color: theme.colorScheme.onTertiaryContainer,
            ),
          ),
          const SizedBox(height: 4),
          Text(
            context.l.requestBannerBody,
            style: theme.textTheme.bodySmall?.copyWith(
              color: theme.colorScheme.onTertiaryContainer,
            ),
          ),
          const SizedBox(height: 4),
          Wrap(
            alignment: WrapAlignment.end,
            spacing: 8,
            children: [
              TextButton(
                onPressed: () async {
                  if (await deleteChat(context, store, chat) &&
                      context.mounted) {
                    Navigator.of(context).pop();
                  }
                },
                child: Text(context.l.delete),
              ),
              if (peer != null)
                TextButton(
                  onPressed: () async {
                    if (await blockPerson(context, store, peer) &&
                        context.mounted) {
                      Navigator.of(context).pop();
                    }
                  },
                  child: Text(context.l.block),
                ),
              FilledButton(
                onPressed: () => acceptChat(context, store, chat.index),
                child: Text(context.l.accept),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// Fila de un mensaje: alineada a un lado u otro, con el nombre y (en los
/// grupos) la foto de quien escribe.
class _Line extends StatelessWidget {
  const _Line({
    required this.store,
    required this.message,
    required this.showSender,
    required this.avatar,
    required this.first,
    required this.child,
  });

  final Store store;
  final lacre.Message message;
  final bool showSender;
  final bool avatar;
  final bool first;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final mine = message.mine;
    final handle = message.from.split(' ').first;
    final content = Column(
      crossAxisAlignment: mine
          ? CrossAxisAlignment.end
          : CrossAxisAlignment.start,
      children: [
        if (showSender)
          Padding(
            padding: const EdgeInsets.only(left: 4, bottom: 2),
            child: Text(
              message.from,
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.primary,
              ),
            ),
          ),
        child,
      ],
    );
    return Padding(
      padding: EdgeInsets.only(top: first ? 6 : 1),
      child: Row(
        mainAxisAlignment: mine
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          if (avatar) ...[
            first
                ? Avatar(name: handle, image: store.avatar(handle), radius: 14)
                : const SizedBox(width: 28),
            const SizedBox(width: 6),
          ],
          Flexible(child: content),
        ],
      ),
    );
  }
}

/// Aviso del chat («ana añadió a carl»), centrado y sin burbuja.
class _Notice extends StatelessWidget {
  const _Notice({required this.message});

  final lacre.Message message;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 6),
      child: Text(
        '${message.text} · ${hhmm(message.sentAt)}',
        textAlign: TextAlign.center,
        style: theme.textTheme.bodySmall?.copyWith(
          color: theme.colorScheme.onSurfaceVariant,
          fontStyle: FontStyle.italic,
        ),
      ),
    );
  }
}

/// Estilo común de las burbujas: las propias a la derecha, en el color
/// principal, con la esquina de abajo de su lado más cerrada.
BoxDecoration _bubble(BuildContext context, bool mine) {
  final scheme = Theme.of(context).colorScheme;
  const r = Radius.circular(18);
  const tail = Radius.circular(4);
  return BoxDecoration(
    color: mine ? scheme.primaryContainer : scheme.surfaceContainerHigh,
    borderRadius: BorderRadius.only(
      topLeft: r,
      topRight: r,
      bottomLeft: mine ? r : tail,
      bottomRight: mine ? tail : r,
    ),
  );
}

double _maxWidth(BuildContext context) =>
    (MediaQuery.of(context).size.width * 0.75).clamp(0, 560);

class _Bubble extends StatelessWidget {
  const _Bubble({required this.message, required this.query});

  final lacre.Message message;

  /// Texto buscado, para resaltarlo.
  final String query;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: _maxWidth(context)),
      child: Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 6),
        decoration: _bubble(context, message.mine),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          mainAxisSize: MainAxisSize.min,
          children: [
            query.trim().isEmpty
                ? SelectableText(message.text)
                : Highlighted(text: message.text, query: query),
            const SizedBox(height: 2),
            Text(
              hhmm(message.sentAt),
              style: theme.textTheme.labelSmall?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Sticker recibido o enviado: sin burbuja. Con pulsación larga (o clic
/// derecho) se puede guardar en la colección propia. En una solicitud de
/// chat no se muestra (D-029).
class _StickerBubble extends StatelessWidget {
  const _StickerBubble({required this.message, required this.hidden});

  final lacre.Message message;
  final bool hidden;

  Future<void> _menu(BuildContext context) async {
    final sticker = message.sticker!;
    final saved = lacre.stickers().any((s) => s.id == sticker.id);
    final keep = await showModalBottomSheet<bool>(
      context: context,
      builder: (context) => SafeArea(
        child: ListTile(
          leading: Icon(saved ? Icons.check : Icons.bookmark_add_outlined),
          title: Text(
            saved ? context.l.alreadyInStickers : context.l.saveToStickers,
          ),
          enabled: !saved,
          onTap: () => Navigator.of(context).pop(true),
        ),
      ),
    );
    if (keep != true || !context.mounted) return;
    final ok = await guard(context, () async {
      lacre.keepSticker(id: sticker.id);
      return true;
    });
    if (ok == true && context.mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(context.l.savedToStickers)));
    }
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (hidden) {
      return Container(
        padding: const EdgeInsets.all(12),
        decoration: _bubble(context, message.mine),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(
              Icons.visibility_off_outlined,
              size: 18,
              color: theme.colorScheme.onSurfaceVariant,
            ),
            const SizedBox(width: 8),
            Text(context.l.stickerHidden, style: theme.textTheme.bodySmall),
          ],
        ),
      );
    }
    return GestureDetector(
      onLongPress: () => _menu(context),
      onSecondaryTap: () => _menu(context),
      child: Column(
        crossAxisAlignment: message.mine
            ? CrossAxisAlignment.end
            : CrossAxisAlignment.start,
        children: [
          Semantics(
            label: context.l.sticker,
            child: StickerImage(id: message.sticker!.id, size: 160),
          ),
          Text(hhmm(message.sentAt), style: theme.textTheme.labelSmall),
        ],
      ),
    );
  }
}

/// Archivo (D-026): se descarga a petición de los dispositivos del grupo que
/// lo tengan y después se abre con la aplicación del sistema. En una
/// solicitud de chat no se puede descargar (D-029).
class _FileBubble extends StatelessWidget {
  const _FileBubble({
    required this.store,
    required this.message,
    required this.locked,
  });

  final Store store;
  final lacre.Message message;
  final bool locked;

  Future<void> _download(BuildContext context, String id) =>
      guard(context, () => store.download(id));

  /// El archivo va cifrado en disco: el núcleo descifra una copia temporal
  /// (se borra al cerrar la app) y se abre esa.
  Future<void> _open(BuildContext context, String id) => guard(
    context,
    () async => openExternal(await lacre.openFile(id: id)),
  );

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final file = message.file!;
    final ready = file.downloaded || store.downloaded.contains(file.id);
    final progress = store.downloads[file.id];
    final Widget action;
    if (ready) {
      action = IconButton(
        tooltip: context.l.open,
        icon: const Icon(Icons.open_in_new),
        onPressed: () => _open(context, file.id),
      );
    } else if (progress != null) {
      action = SizedBox.square(
        dimension: 40,
        child: Padding(
          padding: const EdgeInsets.all(8),
          child: CircularProgressIndicator(
            value: progress > 0 ? progress : null,
          ),
        ),
      );
    } else if (locked) {
      action = IconButton(
        tooltip: context.l.acceptToDownload,
        icon: const Icon(Icons.lock_outline),
        onPressed: null,
      );
    } else {
      action = IconButton(
        tooltip: context.l.download,
        icon: const Icon(Icons.download),
        onPressed: () => _download(context, file.id),
      );
    }
    return ConstrainedBox(
      constraints: BoxConstraints(maxWidth: _maxWidth(context)),
      child: Container(
        padding: const EdgeInsets.fromLTRB(12, 8, 6, 6),
        decoration: _bubble(context, message.mine),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.end,
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.insert_drive_file_outlined),
                const SizedBox(width: 8),
                Flexible(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        file.name,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodyMedium,
                      ),
                      Text(
                        fileSize(file.size.toInt()),
                        style: theme.textTheme.labelSmall,
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 4),
                action,
              ],
            ),
            Padding(
              padding: const EdgeInsets.only(right: 6),
              child: Text(
                hhmm(message.sentAt),
                style: theme.textTheme.labelSmall?.copyWith(
                  color: theme.colorScheme.onSurfaceVariant,
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}
