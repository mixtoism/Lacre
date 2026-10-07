import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:intl/intl.dart';

import '../l10n/l10n.dart';
import '../store.dart';

/// Ejecuta una acción y muestra el error, si lo hay.
Future<T?> guard<T>(BuildContext context, Future<T> Function() action) async {
  try {
    return await action();
  } catch (e) {
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text('$e'),
          backgroundColor: Theme.of(context).colorScheme.error,
        ),
      );
    }
    return null;
  }
}

void copy(BuildContext context, String text, String what) {
  Clipboard.setData(ClipboardData(text: text));
  ScaffoldMessenger.of(context)
      .showSnackBar(SnackBar(content: Text(context.l.copied(what))));
}

/// Tamaño legible: «3,2 MB».
String fileSize(int bytes) {
  if (bytes < 1024) return '$bytes B';
  const units = ['KB', 'MB', 'GB'];
  var v = bytes / 1024;
  var u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  final n = NumberFormat(v < 10 ? '0.0' : '0').format(v);
  return '$n ${units[u]}';
}

/// En Linux, Flutter busca los emojis que faltan en la fuente por su cuenta
/// y suele acabar en una de emojis en blanco y negro (Noto Emoji, Symbola)
/// aunque esté instalada la de color. Con esta lista usa primero la fuente
/// del sistema y después la de color. `sans-serif` va delante para que los
/// espacios no salgan de la fuente de emojis (son muy anchos).
final List<String>? fontFallback = Platform.isLinux
    ? const ['sans-serif', 'Noto Color Emoji']
    : null;

/// Abre un archivo con la aplicación del sistema. En Android, a través del
/// FileProvider de la app (ver LacreApp.kt).
Future<void> openExternal(String path) async {
  if (Platform.isAndroid) {
    final ok = await sistema.invokeMethod<bool>('abrir', {'path': path});
    if (ok != true) throw systemL10n.noAppToOpen;
    return;
  }
  final (cmd, args) = Platform.isWindows
      ? ('explorer', [path])
      : Platform.isMacOS
      ? ('open', [path])
      : ('xdg-open', [path]);
  final r = await Process.run(cmd, args);
  if (r.exitCode != 0 && !Platform.isWindows) {
    throw systemL10n.cantOpenFile;
  }
}

String hhmm(int unixSeconds) {
  final t = DateTime.fromMillisecondsSinceEpoch(unixSeconds * 1000);
  final now = DateTime.now();
  final time = DateFormat.Hm().format(t);
  if (t.year == now.year && t.month == now.month && t.day == now.day) {
    return time;
  }
  return '${DateFormat.Md().format(t)} $time';
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
      child: Row(
        children: [
          Expanded(
            child: SelectableText(
              text,
              style: const TextStyle(fontFamily: 'monospace'),
            ),
          ),
          IconButton(
            tooltip: context.l.copy,
            icon: const Icon(Icons.copy),
            onPressed: () => copy(context, text, what),
          ),
        ],
      ),
    );
  }
}

/// Foto de perfil redonda; sin foto, la inicial sobre un color que sale del
/// nombre (siempre el mismo para la misma persona).
class Avatar extends StatelessWidget {
  const Avatar({
    super.key,
    required this.name,
    this.path,
    this.radius = 20,
    this.group = false,
  });

  /// Handle o nombre del chat.
  final String name;
  final String? path;
  final double radius;

  /// Chat de grupo sin foto: icono de grupo en vez de inicial.
  final bool group;

  @override
  Widget build(BuildContext context) {
    final path = this.path;
    if (path != null) {
      return CircleAvatar(
        radius: radius,
        backgroundImage: FileImage(File(path)),
      );
    }
    final label = name.split('#').first.trim();
    final hue = (name.hashCode % 360).abs().toDouble();
    final dark = Theme.of(context).brightness == Brightness.dark;
    final bg = HSLColor.fromAHSL(1, hue, 0.45, dark ? 0.35 : 0.75).toColor();
    final fg = HSLColor.fromAHSL(1, hue, 0.6, dark ? 0.9 : 0.2).toColor();
    return CircleAvatar(
      radius: radius,
      backgroundColor: bg,
      foregroundColor: fg,
      child: group
          ? Icon(Icons.group, size: radius)
          : Text(
              label.isEmpty ? '?' : label.characters.first.toUpperCase(),
              style: TextStyle(
                fontSize: radius * 0.9,
                fontWeight: FontWeight.w600,
              ),
            ),
    );
  }
}

/// Texto con las apariciones de `query` resaltadas (sin distinguir
/// mayúsculas ni tildes, como la búsqueda del núcleo).
class Highlighted extends StatelessWidget {
  const Highlighted({
    super.key,
    required this.text,
    required this.query,
    this.style,
    this.maxLines,
  });

  final String text;
  final String query;
  final TextStyle? style;
  final int? maxLines;

  @override
  Widget build(BuildContext context) {
    final q = fold(query.trim());
    final base = style ?? DefaultTextStyle.of(context).style;
    if (q.isEmpty) {
      return Text(
        text,
        style: base,
        maxLines: maxLines,
        overflow: TextOverflow.ellipsis,
      );
    }
    // `fold` conserva la longitud carácter a carácter, así que las
    // posiciones valen para el texto original.
    final folded = fold(text);
    final mark = base.copyWith(
      backgroundColor: Theme.of(context).colorScheme.tertiaryContainer,
      color: Theme.of(context).colorScheme.onTertiaryContainer,
      fontWeight: FontWeight.w600,
    );
    final spans = <TextSpan>[];
    var at = 0;
    while (true) {
      final i = folded.indexOf(q, at);
      if (i < 0) break;
      if (i > at) spans.add(TextSpan(text: text.substring(at, i)));
      spans.add(TextSpan(text: text.substring(i, i + q.length), style: mark));
      at = i + q.length;
    }
    spans.add(TextSpan(text: text.substring(at)));
    return Text.rich(
      TextSpan(style: base, children: spans),
      maxLines: maxLines,
      overflow: maxLines == null ? null : TextOverflow.ellipsis,
    );
  }
}

/// Minúsculas y sin tildes, como `people::fold` en el núcleo. Solo cambia
/// letras sueltas, así que el resultado tiene la misma longitud.
String fold(String s) {
  const from = 'áàâäãéèêëíìîïóòôöõúùûüç';
  const to = 'aaaaaeeeeiiiiooooouuuuc';
  final lower = s.toLowerCase();
  if (lower.length != s.length) return s;
  final b = StringBuffer();
  for (final ch in lower.split('')) {
    final i = from.indexOf(ch);
    b.write(i < 0 ? ch : to[i]);
  }
  return b.toString();
}

/// Pregunta antes de una acción que no se puede deshacer.
Future<bool> confirm(
  BuildContext context, {
  required String title,
  required String body,
  required String action,
}) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(title),
      content: Text(body),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(context.l.cancel),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(action),
        ),
      ],
    ),
  );
  return ok == true;
}

/// Pregunta qué deben decir los avisos de mensajes nuevos (D-035).
Future<void> chooseNoticeContent(BuildContext context, Store store) async {
  var choice = store.noticeContent ?? NoticeContent.none;
  final ok = await showDialog<bool>(
    context: context,
    barrierDismissible: store.noticeContent != null,
    builder: (context) => StatefulBuilder(
      builder: (context, setState) => AlertDialog(
        title: Text(context.l.messageNotices),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(context.l.messageNoticesBody),
            const SizedBox(height: 8),
            RadioGroup<NoticeContent>(
              groupValue: choice,
              onChanged: (v) => setState(() => choice = v!),
              child: Column(
                children: [
                  for (final c in NoticeContent.values.reversed)
                    RadioListTile<NoticeContent>(
                      value: c,
                      title: Text(c.label(context.l)),
                      contentPadding: EdgeInsets.zero,
                    ),
                ],
              ),
            ),
          ],
        ),
        actions: [
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l.accept),
          ),
        ],
      ),
    ),
  );
  if (ok == true) await store.setNoticeContent(choice);
}
