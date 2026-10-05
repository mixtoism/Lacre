import 'dart:async';

import 'package:flutter/material.dart';

import '../src/rust/api/p2m.dart' as p2m;
import '../store.dart';
import 'common.dart';

class ChatPage extends StatefulWidget {
  const ChatPage({super.key, required this.store, required this.index, required this.title});

  final Store store;
  final int index;
  final String title;

  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final _input = TextEditingController();
  final _focus = FocusNode();
  late List<p2m.Message> _messages;
  late StreamSubscription<(int, p2m.Message)> _sub;
  bool _sending = false;

  @override
  void initState() {
    super.initState();
    _messages = p2m.messages(index: widget.index);
    _sub = widget.store.messages.stream.where((m) => m.$1 == widget.index).listen((m) {
      setState(() => _messages.add(m.$2));
    });
  }

  @override
  void dispose() {
    _sub.cancel();
    _input.dispose();
    _focus.dispose();
    super.dispose();
  }

  Future<void> _send() async {
    final text = _input.text.trim();
    if (text.isEmpty || _sending) return;
    setState(() => _sending = true);
    final r = await guard(context, () => p2m.send(index: widget.index, text: text));
    if (!mounted) return;
    setState(() => _sending = false);
    if (r != null) {
      _input.clear();
      if (r.failed > 0) {
        ScaffoldMessenger.of(context).showSnackBar(SnackBar(
          content: Text('No se pudo entregar a ${r.failed} dispositivo(s); se reintentará'),
        ));
      }
    }
    _focus.requestFocus();
  }

  @override
  Widget build(BuildContext context) {
    final chat = widget.store.chats.where((c) => c.index == widget.index).firstOrNull;
    final active = chat?.active ?? true;
    final group = (chat?.devices ?? 0) > 3;
    return Scaffold(
      appBar: AppBar(title: Text(widget.title)),
      body: Column(children: [
        Expanded(
          child: ListView.builder(
            reverse: true,
            padding: const EdgeInsets.all(12),
            itemCount: _messages.length,
            itemBuilder: (context, i) {
              final m = _messages[_messages.length - 1 - i];
              return _Bubble(message: m, showSender: group || !m.mine);
            },
          ),
        ),
        const Divider(height: 1),
        SafeArea(
          child: Padding(
            padding: const EdgeInsets.all(8),
            child: active
                ? Row(children: [
                    Expanded(
                      child: TextField(
                        controller: _input,
                        focusNode: _focus,
                        autofocus: true,
                        minLines: 1,
                        maxLines: 5,
                        textInputAction: TextInputAction.send,
                        onSubmitted: (_) => _send(),
                        decoration: const InputDecoration(
                          hintText: 'Mensaje',
                          border: OutlineInputBorder(borderRadius: BorderRadius.all(Radius.circular(24))),
                          contentPadding: EdgeInsets.symmetric(horizontal: 16, vertical: 10),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    IconButton.filled(
                      tooltip: 'Enviar',
                      onPressed: _sending ? null : _send,
                      icon: const Icon(Icons.send),
                    ),
                  ])
                : const Text('Ya no participas en este chat.'),
          ),
        ),
      ]),
    );
  }
}

class _Bubble extends StatelessWidget {
  const _Bubble({required this.message, required this.showSender});

  final p2m.Message message;
  final bool showSender;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final mine = message.mine;
    return Align(
      alignment: mine ? Alignment.centerRight : Alignment.centerLeft,
      child: ConstrainedBox(
        constraints: BoxConstraints(maxWidth: MediaQuery.of(context).size.width * 0.75),
        child: Container(
          margin: const EdgeInsets.symmetric(vertical: 3),
          padding: const EdgeInsets.fromLTRB(12, 8, 12, 6),
          decoration: BoxDecoration(
            color: mine ? scheme.primaryContainer : scheme.surfaceContainerHigh,
            borderRadius: BorderRadius.circular(16),
          ),
          child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
            if (showSender)
              Text(message.from,
                  style: Theme.of(context).textTheme.labelSmall?.copyWith(color: scheme.primary)),
            SelectableText(message.text),
            Align(
              alignment: Alignment.bottomRight,
              child: Text(hhmm(message.sentAt), style: Theme.of(context).textTheme.labelSmall),
            ),
          ]),
        ),
      ),
    );
  }
}
