import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:emoji_picker_flutter/emoji_picker_flutter.dart';
import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/p2m.dart' as p2m;
import 'common.dart';

/// Panel bajo el campo de texto: emojis y stickers propios (D-025). Todo es
/// local; no se consulta ningún servicio externo.
class ExpressionPanel extends StatelessWidget {
  const ExpressionPanel({
    super.key,
    required this.input,
    required this.onSticker,
  });

  /// Campo de texto donde se insertan los emojis.
  final TextEditingController input;
  final ValueChanged<p2m.Sticker> onSticker;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return SizedBox(
      height: 300,
      child: DefaultTabController(
        length: 2,
        child: Column(
          children: [
            TabBar(
              tabs: [
                Tab(
                  icon: const Icon(Icons.emoji_emotions_outlined),
                  text: context.l.emoji,
                ),
                Tab(
                  icon: const Icon(Icons.sticky_note_2_outlined),
                  text: context.l.stickers,
                ),
              ],
            ),
            Expanded(
              child: TabBarView(
                children: [
                  EmojiPicker(
                    textEditingController: input,
                    config: Config(
                      height: null,
                      locale: Localizations.localeOf(context),
                      checkPlatformCompatibility: true,
                      emojiTextStyle: TextStyle(
                        fontFamilyFallback: fontFallback,
                      ),
                      emojiViewConfig: EmojiViewConfig(
                        columns: 9,
                        emojiSizeMax: 28,
                        backgroundColor: scheme.surface,
                        noRecents: Text(
                          context.l.noRecents,
                          style: TextStyle(color: scheme.onSurfaceVariant),
                        ),
                      ),
                      categoryViewConfig: CategoryViewConfig(
                        backgroundColor: scheme.surface,
                        indicatorColor: scheme.primary,
                        iconColorSelected: scheme.primary,
                        backspaceColor: scheme.primary,
                      ),
                      bottomActionBarConfig: const BottomActionBarConfig(
                        enabled: false,
                      ),
                      searchViewConfig: SearchViewConfig(
                        backgroundColor: scheme.surface,
                        hintText: context.l.searchEmoji,
                      ),
                    ),
                  ),
                  StickerGrid(onSticker: onSticker),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Colección propia de stickers. El primer botón importa imágenes.
class StickerGrid extends StatefulWidget {
  const StickerGrid({super.key, required this.onSticker});

  final ValueChanged<p2m.Sticker> onSticker;

  @override
  State<StickerGrid> createState() => _StickerGridState();
}

class _StickerGridState extends State<StickerGrid> {
  late List<p2m.Sticker> _stickers = p2m.stickers();
  bool _importing = false;

  Future<void> _import() async {
    final files = await openFiles(
      acceptedTypeGroups: [
        XTypeGroup(
          label: context.l.images,
          extensions: ['png', 'gif', 'webp', 'jpg', 'jpeg'],
          mimeTypes: ['image/png', 'image/gif', 'image/webp', 'image/jpeg'],
        ),
      ],
    );
    if (files.isEmpty || !mounted) return;
    setState(() => _importing = true);
    final messenger = ScaffoldMessenger.of(context);
    final errors = <String>[];
    for (final f in files) {
      try {
        p2m.addSticker(bytes: await fitSticker(await f.readAsBytes()));
      } catch (e) {
        errors.add('${f.name}: ${e is ArgumentError ? e.message : e}');
      }
    }
    if (!mounted) return;
    setState(() {
      _importing = false;
      _stickers = p2m.stickers();
    });
    if (errors.isNotEmpty) {
      messenger.showSnackBar(SnackBar(content: Text(errors.join('\n'))));
    }
  }

  Future<void> _remove(p2m.Sticker s) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.removeStickerTitle),
        content: Image.file(File(s.path), height: 120, cacheHeight: 240),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: Text(context.l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: Text(context.l.remove),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    await guard(context, () async => p2m.removeSticker(id: s.id));
    if (mounted) setState(() => _stickers = p2m.stickers());
  }

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    return GridView.builder(
      padding: const EdgeInsets.all(8),
      gridDelegate: const SliverGridDelegateWithMaxCrossAxisExtent(
        maxCrossAxisExtent: 96,
        mainAxisSpacing: 8,
        crossAxisSpacing: 8,
      ),
      itemCount: _stickers.length + 1,
      itemBuilder: (context, i) {
        if (i == 0) {
          return Tooltip(
            message: context.l.addStickers,
            child: InkWell(
              borderRadius: BorderRadius.circular(12),
              onTap: _importing ? null : _import,
              child: Ink(
                decoration: BoxDecoration(
                  border: Border.all(color: scheme.outlineVariant),
                  borderRadius: BorderRadius.circular(12),
                ),
                child: Center(
                  child: _importing
                      ? const CircularProgressIndicator()
                      : Icon(Icons.add_photo_alternate, color: scheme.primary),
                ),
              ),
            ),
          );
        }
        final s = _stickers[i - 1];
        return Tooltip(
          message: context.l.sendSticker,
          child: InkWell(
            key: ValueKey(s.id),
            borderRadius: BorderRadius.circular(12),
            onTap: () => widget.onSticker(s),
            onLongPress: () => _remove(s),
            onSecondaryTap: () => _remove(s),
            child: Padding(
              padding: const EdgeInsets.all(4),
              child: StickerImage(path: s.path, size: 88),
            ),
          ),
        );
      },
    );
  }
}

/// Imagen de un sticker. Se decodifica a tamaño reducido; las dimensiones
/// ya las ha limitado el núcleo (`sticker::check`).
class StickerImage extends StatelessWidget {
  const StickerImage({super.key, required this.path, required this.size});

  final String path;
  final double size;

  @override
  Widget build(BuildContext context) {
    final px = (size * MediaQuery.devicePixelRatioOf(context)).round();
    return Image.file(
      File(path),
      width: size,
      height: size,
      fit: BoxFit.contain,
      cacheWidth: px,
      gaplessPlayback: true,
      errorBuilder: (_, _, _) =>
          Icon(Icons.broken_image_outlined, size: size / 2),
    );
  }
}

/// Ajusta una imagen al límite de los stickers: si ocupa o mide demasiado y
/// no es animada, la reduce a 512 px (o menos) y la guarda como PNG.
Future<Uint8List> fitSticker(Uint8List bytes) async {
  final max = p2m.maxStickerBytes();
  final ui.ImageDescriptor desc;
  try {
    desc = await ui.ImageDescriptor.encoded(
      await ui.ImmutableBuffer.fromUint8List(bytes),
    );
  } catch (_) {
    throw ArgumentError(systemL10n.notAnImage);
  }
  final side = desc.width > desc.height ? desc.width : desc.height;
  if (bytes.length <= max && side <= 512) return bytes;
  final codec = await ui.instantiateImageCodec(bytes);
  final animated = codec.frameCount > 1;
  codec.dispose();
  if (animated) {
    throw ArgumentError(
      systemL10n.stickerAnimatedTooBig(bytes.length ~/ 1024, side, max ~/ 1024),
    );
  }
  for (final target in [512, 384, 256, 192]) {
    final scale = target / side;
    final small = await desc.instantiateCodec(
      targetWidth: (desc.width * (scale < 1 ? scale : 1)).round(),
      targetHeight: (desc.height * (scale < 1 ? scale : 1)).round(),
    );
    final frame = await small.getNextFrame();
    final png = await frame.image.toByteData(format: ui.ImageByteFormat.png);
    frame.image.dispose();
    small.dispose();
    if (png != null && png.lengthInBytes <= max) {
      desc.dispose();
      return png.buffer.asUint8List();
    }
  }
  desc.dispose();
  throw ArgumentError(systemL10n.stickerCantShrink(max ~/ 1024));
}
