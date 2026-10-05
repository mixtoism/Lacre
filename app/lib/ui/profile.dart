import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../src/rust/api/p2m.dart' as p2m;
import '../store.dart';
import 'common.dart';

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
  }

  Future<void> _loadQr() async {
    final qr = await guard(context, p2m.qrPayload);
    if (mounted) setState(() => _qr = qr);
  }

  Future<void> _link() async {
    final code = TextEditingController();
    final label = TextEditingController(text: 'portátil');
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Vincular dispositivo'),
        content: SizedBox(
          width: 420,
          child: Column(mainAxisSize: MainAxisSize.min, children: [
            const Text('En el dispositivo nuevo elige «Vincular dispositivo» y pega aquí su código.'),
            const SizedBox(height: 16),
            TextField(
              controller: code,
              autofocus: true,
              decoration: const InputDecoration(labelText: 'Código del dispositivo', border: OutlineInputBorder()),
            ),
            const SizedBox(height: 12),
            TextField(
              controller: label,
              decoration: const InputDecoration(labelText: 'Nombre del dispositivo', border: OutlineInputBorder()),
            ),
          ]),
        ),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancelar')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Vincular')),
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
        title: Text('¿Revocar «${d.label}»?'),
        content: const Text('Dejará de recibir mensajes y se le expulsará de todos los chats.'),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: const Text('Cancelar')),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: const Text('Revocar')),
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
      appBar: AppBar(title: const Text('Perfil')),
      body: ListenableBuilder(
        listenable: widget.store,
        builder: (context, _) {
          final status = widget.store.status;
          return ListView(padding: const EdgeInsets.all(24), children: [
            Center(child: Text(profile.handle!, style: text.headlineSmall)),
            const SizedBox(height: 16),
            Center(
              child: _qr == null
                  ? const SizedBox.square(dimension: 240, child: Center(child: CircularProgressIndicator()))
                  : Container(
                      color: Colors.white,
                      padding: const EdgeInsets.all(12),
                      child: QrImageView(data: _qr!, size: 240),
                    ),
            ),
            const SizedBox(height: 8),
            Text(
              'Quien escanee o pegue este código podrá escribirte. Incluye tu clave pública y, '
              'si ya está publicado, tu servicio onion para encontrarte desde cualquier red.',
              style: text.bodySmall,
              textAlign: TextAlign.center,
            ),
            const SizedBox(height: 8),
            if (_qr != null) CodeBox(text: _qr!, what: 'Código QR'),
            Align(
              alignment: Alignment.centerRight,
              child: TextButton.icon(onPressed: _loadQr, icon: const Icon(Icons.refresh), label: const Text('Actualizar')),
            ),
            const Divider(height: 32),
            Text('Red', style: text.titleMedium),
            ListTile(
              leading: const Icon(Icons.hub_outlined),
              title: Text('${status?.peers ?? 0} peers conectados'),
            ),
            ListTile(
              leading: const Icon(Icons.security),
              title: Text(status?.onion == null ? 'Servicio onion: publicando…' : 'Servicio onion publicado'),
              subtitle: status?.onion == null ? null : Text(status!.onion!, style: const TextStyle(fontFamily: 'monospace')),
            ),
            if (widget.store.relay != null)
              ListTile(
                leading: const Icon(Icons.swap_horiz),
                title: const Text('Alcanzable por relay'),
                subtitle: Text(widget.store.relay!, style: const TextStyle(fontFamily: 'monospace')),
              ),
            const Divider(height: 32),
            Row(children: [
              Expanded(child: Text('Dispositivos', style: text.titleMedium)),
              if (profile.primary)
                TextButton.icon(onPressed: _link, icon: const Icon(Icons.add_link), label: const Text('Vincular')),
            ]),
            for (final d in _devices)
              ListTile(
                leading: Icon(d.thisDevice ? Icons.smartphone : Icons.devices_other),
                title: Text(d.thisDevice ? '${d.label} (este)' : d.label),
                subtitle: Text(d.code, style: const TextStyle(fontFamily: 'monospace'), maxLines: 1, overflow: TextOverflow.ellipsis),
                trailing: profile.primary && !d.thisDevice
                    ? IconButton(tooltip: 'Revocar', icon: const Icon(Icons.link_off), onPressed: () => _unlink(d))
                    : null,
              ),
          ]);
        },
      ),
    );
  }
}
