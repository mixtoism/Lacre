import 'dart:io';

import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:file_selector/file_selector.dart';
import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/p2m.dart' as p2m;
import '../store.dart';
import 'common.dart';
import 'scan.dart';

class ProfilePage extends StatefulWidget {
  const ProfilePage({super.key, required this.store});

  final Store store;

  @override
  State<ProfilePage> createState() => _ProfilePageState();
}

class _ProfilePageState extends State<ProfilePage> {
  String? _qr;
  late List<p2m.Device> _devices;

  @override
  void initState() {
    super.initState();
    _devices = p2m.devices();
    _loadQr();
    widget.store.addListener(_onStore);
    _loadBattery();
    // Al volver de los ajustes de batería.
    _lifecycle = AppLifecycleListener(onResume: _loadBattery);
  }

  /// Android: exenta de la optimización de batería (`null`: no aplica).
  bool? _batteryFree;
  late final AppLifecycleListener _lifecycle;

  Future<void> _loadBattery() async {
    if (!Platform.isAndroid) return;
    final free = await widget.store.batteryFree();
    if (mounted) setState(() => _batteryFree = free);
  }

  @override
  void dispose() {
    _lifecycle.dispose();
    widget.store.removeListener(_onStore);
    super.dispose();
  }

  /// Onion con el que se generó `_qr`.
  String? _qrOnion;

  /// El onion se publica un rato después de arrancar: en cuanto aparece (o
  /// cambia), se regenera el código para que lo lleve.
  void _onStore() {
    final onion = widget.store.status?.onion;
    if (onion != null && onion != _qrOnion) _loadQr();
  }

  Future<void> _loadQr() async {
    _qrOnion = widget.store.status?.onion;
    final qr = await guard(context, p2m.qrPayload);
    if (mounted) setState(() => _qr = qr);
  }

  /// Elige una imagen, la recorta en cuadrado y la reduce hasta que quepa
  /// en el límite del núcleo (128 KB, D-030).
  Future<void> _pickAvatar() async {
    final file = await openFile(
      acceptedTypeGroups: [
        XTypeGroup(
          label: context.l.images,
          extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp'],
          mimeTypes: ['image/*'],
          uniformTypeIdentifiers: ['public.image'],
        ),
      ],
    );
    if (file == null || !mounted) return;
    setState(() => _busy = true);
    await guard(context, () async {
      final png = await squareAvatar(await file.readAsBytes());
      await widget.store.setAvatar(png);
    });
    if (mounted) setState(() => _busy = false);
  }

  Future<void> _removeAvatar() async {
    setState(() => _busy = true);
    await guard(context, () => widget.store.setAvatar(null));
    if (mounted) setState(() => _busy = false);
  }

  /// Cambiando la foto.
  bool _busy = false;

  Future<void> _link() async {
    final code = TextEditingController();
    final label = TextEditingController(text: context.l.defaultDeviceLabel);
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.linkDevice),
        content: SizedBox(
          width: 420,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  canScan
                      ? context.l.linkDialogScan
                      : context.l.linkDialogPaste,
                ),
                const SizedBox(height: 16),
                TextField(
                  controller: code,
                  autofocus: true,
                  decoration: InputDecoration(
                    labelText: context.l.deviceCode,
                    border: const OutlineInputBorder(),
                    suffixIcon: canScan
                        ? IconButton(
                            tooltip: context.l.scanQr,
                            icon: const Icon(Icons.qr_code_scanner),
                            onPressed: () async {
                              final v = await scanQr(
                                context,
                                title: context.l.scanNewDeviceQr,
                                accept: (v) => !v.startsWith('p2m:'),
                              );
                              if (v != null) code.text = v;
                            },
                          )
                        : null,
                  ),
                ),
                const SizedBox(height: 12),
                TextField(
                  controller: label,
                  decoration: InputDecoration(
                    labelText: context.l.deviceName,
                    border: const OutlineInputBorder(),
                  ),
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l.link),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    final done = await guard(context, () async {
      await p2m.linkDevice(code: code.text, label: label.text.trim());
      return true;
    });
    if (done == true && mounted) setState(() => _devices = p2m.devices());
  }

  Future<void> _unlink(p2m.Device d) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.revokeTitle(d.label)),
        content: Text(context.l.revokeBody),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(context.l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(context.l.revoke),
          ),
        ],
      ),
    );
    if (ok != true || !mounted) return;
    final done = await guard(context, () async {
      await p2m.unlinkDevice(code: d.code);
      return true;
    });
    if (done == true && mounted) setState(() => _devices = p2m.devices());
  }

  @override
  Widget build(BuildContext context) {
    final profile = widget.store.profile!;
    final text = Theme.of(context).textTheme;
    return Scaffold(
      appBar: AppBar(title: Text(context.l.profile)),
      body: ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final status = widget.store.status;
          final blocked = p2m.blocked();
          return ListView(
            padding: const EdgeInsets.all(24),
            children: [
              Center(
                child: Stack(
                  children: [
                    Avatar(
                      name: profile.handle!,
                      path: widget.store.avatar(profile.handle!),
                      radius: 56,
                    ),
                    if (_busy)
                      const Positioned.fill(child: CircularProgressIndicator()),
                    Positioned(
                      right: 0,
                      bottom: 0,
                      child: IconButton.filled(
                        tooltip: context.l.changePhoto,
                        icon: const Icon(Icons.photo_camera_outlined),
                        onPressed: _busy ? null : _pickAvatar,
                      ),
                    ),
                  ],
                ),
              ),
              if (widget.store.avatar(profile.handle!) != null)
                Center(
                  child: TextButton(
                    onPressed: _busy ? null : _removeAvatar,
                    child: Text(context.l.removePhoto),
                  ),
                ),
              const SizedBox(height: 8),
              Center(
                child: Text(
                  profile.handle!.split('#').first,
                  style: text.headlineSmall,
                ),
              ),
              Center(
                child: Text(
                  '#${profile.handle!.split('#').last}',
                  style: text.bodyMedium?.copyWith(
                    color: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
              const SizedBox(height: 4),
              Text(
                context.l.photoVisibility,
                style: text.bodySmall,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 16),
              Center(
                child: _qr == null
                    ? const SizedBox.square(
                        dimension: 240,
                        child: Center(child: CircularProgressIndicator()),
                      )
                    : Container(
                        color: Colors.white,
                        padding: const EdgeInsets.all(12),
                        child: QrImageView(data: _qr!, size: 240),
                      ),
              ),
              const SizedBox(height: 8),
              Text(
                context.l.qrExplanation,
                style: text.bodySmall,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              if (_qr != null) CodeBox(text: _qr!, what: context.l.qrCode),
              Align(
                alignment: Alignment.centerRight,
                child: TextButton.icon(
                  onPressed: _loadQr,
                  icon: const Icon(Icons.refresh),
                  label: Text(context.l.refresh),
                ),
              ),
              const Divider(height: 32),
              Text(context.l.network, style: text.titleMedium),
              ListTile(
                leading: const Icon(Icons.hub_outlined),
                title: Text(context.l.peersConnected(status?.peers ?? 0)),
              ),
              ListTile(
                leading: const Icon(Icons.security),
                title: Text(
                  status?.onion == null
                      ? context.l.onionPublishing
                      : context.l.onionPublished,
                ),
                subtitle: status?.onion == null
                    ? null
                    : Text(
                        status!.onion!,
                        style: const TextStyle(fontFamily: 'monospace'),
                      ),
              ),
              if (widget.store.relay != null)
                ListTile(
                  leading: const Icon(Icons.swap_horiz),
                  title: Text(context.l.reachableByRelay),
                  subtitle: Text(
                    widget.store.relay!,
                    style: const TextStyle(fontFamily: 'monospace'),
                  ),
                ),
              const Divider(height: 32),
              Text(context.l.thisDevice, style: text.titleMedium),
              ListTile(
                leading: Icon(
                  profile.encrypted ? Icons.lock_outline : Icons.lock_open,
                ),
                title: Text(
                  profile.encrypted
                      ? context.l.dataEncrypted
                      : context.l.dataNotEncrypted,
                ),
                subtitle: Text(
                  profile.encrypted
                      ? context.l.keyInKeystore
                      : context.l.noKeystore,
                ),
              ),
              ListTile(
                leading: const Icon(Icons.notifications_outlined),
                title: Text(context.l.messageNotices),
                subtitle: Text(
                  (widget.store.noticeContent ?? NoticeContent.none).label(
                    context.l,
                  ),
                ),
                onTap: () async {
                  await chooseNoticeContent(context, widget.store);
                  if (mounted) setState(() {});
                },
              ),
              if (_batteryFree != null)
                ListTile(
                  leading: const Icon(Icons.battery_saver_outlined),
                  title: Text(
                    _batteryFree!
                        ? context.l.batteryFree
                        : context.l.batteryRestricted,
                  ),
                  subtitle: Text(context.l.batteryExplanation),
                  trailing: _batteryFree!
                      ? null
                      : TextButton(
                          onPressed: widget.store.askBattery,
                          child: Text(context.l.allow),
                        ),
                ),
              const Divider(height: 32),
              Row(
                children: [
                  Expanded(
                    child: Text(context.l.devices, style: text.titleMedium),
                  ),
                  if (profile.primary)
                    TextButton.icon(
                      onPressed: _link,
                      icon: const Icon(Icons.add_link),
                      label: Text(context.l.link),
                    ),
                ],
              ),
              for (final d in _devices)
                ListTile(
                  leading: Icon(
                    d.thisDevice ? Icons.smartphone : Icons.devices_other,
                  ),
                  title: Text(
                    d.thisDevice ? context.l.thisDeviceLabel(d.label) : d.label,
                  ),
                  subtitle: Text(
                    d.code,
                    style: const TextStyle(fontFamily: 'monospace'),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  trailing: profile.primary && !d.thisDevice
                      ? IconButton(
                          tooltip: context.l.revoke,
                          icon: const Icon(Icons.link_off),
                          onPressed: () => _unlink(d),
                        )
                      : null,
                ),
              const Divider(height: 32),
              Text(context.l.blocked, style: text.titleMedium),
              if (blocked.isEmpty)
                ListTile(
                  leading: const Icon(Icons.block),
                  title: Text(context.l.noBlocked),
                ),
              for (final h in blocked)
                ListTile(
                  leading: Avatar(name: h),
                  title: Text(h.split('#').first),
                  subtitle: Text(h),
                  trailing: TextButton(
                    onPressed: () =>
                        guard(context, () async => widget.store.unblock(h)),
                    child: Text(context.l.unblock),
                  ),
                ),
            ],
          );
        },
      ),
    );
  }
}

/// Recorta la imagen en un cuadrado centrado y la guarda como PNG, cada vez
/// más pequeña hasta que ocupe menos de 128 KB.
Future<Uint8List> squareAvatar(Uint8List bytes) async {
  final codec = await ui.instantiateImageCodec(bytes);
  final image = (await codec.getNextFrame()).image;
  codec.dispose();
  for (final side in const [256, 192, 128, 96, 64]) {
    final crop = image.width < image.height ? image.width : image.height;
    final recorder = ui.PictureRecorder();
    ui.Canvas(recorder).drawImageRect(
      image,
      Rect.fromLTWH(
        (image.width - crop) / 2,
        (image.height - crop) / 2,
        crop.toDouble(),
        crop.toDouble(),
      ),
      Rect.fromLTWH(0, 0, side.toDouble(), side.toDouble()),
      Paint()..filterQuality = FilterQuality.high,
    );
    final out = await recorder.endRecording().toImage(side, side);
    final png = await out.toByteData(format: ui.ImageByteFormat.png);
    out.dispose();
    if (png != null && png.lengthInBytes <= 128 * 1024) {
      image.dispose();
      return png.buffer.asUint8List();
    }
  }
  image.dispose();
  throw systemL10n.cantShrinkImage;
}
