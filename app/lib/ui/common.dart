import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// Ejecuta una acción y muestra el error, si lo hay.
Future<T?> guard<T>(BuildContext context, Future<T> Function() action) async {
  try {
    return await action();
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(SnackBar(
        content: Text('$e'),
        backgroundColor: Theme.of(context).colorScheme.error,
      ));
    }
    return null;
  }
}

void copy(BuildContext context, String text, String what) {
  Clipboard.setData(ClipboardData(text: text));
  ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text('$what copiado')));
}

String hhmm(int unixSeconds) {
  final t = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  final now = DateTime.now();
  String two(int n) => n.toString().padLeft(2, '0');
  final time = '${two(t.hour)}:${two(t.minute)}';
  if (t.year == now.year && t.month == now.month && t.day == now.day) return time;
  return '${two(t.day)}/${two(t.month)} $time';
}

/// Código copiable en tipografía monoespaciada.
class CodeBox extends StatelessWidget {
  const CodeBox({super.key, required this.text, required this.what});

  final String text;
  final String what;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return Container(
      decoration: BoxDecoration(
        color: scheme.surfaceContainerHighest,
        borderRadius: BorderRadius.circular(12),
      ),
      padding: const EdgeInsets.fromLTRB(16, 8, 8, 8),
      child: Row(children: [
        Expanded(child: SelectableText(text, style: const TextStyle(fontFamily: 'monospace'))),
        IconButton(
          tooltip: 'Copiar',
          icon: const Icon(Icons.copy),
          onPressed: () => copy(context, text, what),
        ),
      ]),
    );
  }
}
