import 'dart:async';

import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/lacre.dart' as lacre;
import '../store.dart';
import 'common.dart';
import 'scan.dart';

// Vinculación de dispositivos (D-037). El que no tiene cámara (el
// escritorio) muestra el QR y el móvil lo escanea, sea cual sea el
// primario: el primario muestra una invitación y el nuevo, su código.

/// Las direcciones del código cambian cuando se publica el servicio onion.
const _refreshEvery = Duration(seconds: 10);

/// Nombre con el que se presenta un dispositivo nuevo.
String newDeviceLabel(BuildContext context) =>
    canScan ? context.l.labelPhone : context.l.labelComputer;

bool _reachableAnywhere(String code) => code.contains('/onion3/');

/// Explica por qué no se pudo vincular. Devuelve `true` si se vinculó.
///
/// Si ha ido bien y `showConfirm`, enseña el código de confirmación (D-047)
/// que el otro dispositivo también muestra.
Future<bool> explainLink(BuildContext context, lacre.LinkResult r,
    {bool showConfirm = true}) async {
  if (r case lacre.LinkResult_Done(:final confirm)) {
    if (showConfirm && context.mounted) {
      await showDialog<void>(
        context: context,
        builder: (context) => AlertDialog(
          icon: const Icon(Icons.verified_user_outlined),
          title: Text(context.l.linkConfirmTitle),
          content: Text(context.l.linkConfirmBody(confirm)),
          actions: [
            FilledButton(
              onPressed: () => Navigator.pop(context),
              child: Text(context.l.ok),
            ),
          ],
        ),
      );
    }
    return true;
  }
  final text = switch (r) {
    lacre.LinkResult_Done() => null,
    lacre.LinkResult_BadCode() => context.l.linkBadCode,
    lacre.LinkResult_Unreachable() => context.l.linkUnreachable,
    lacre.LinkResult_Refused(:final reason) => context.l.linkRefused(reason),
  };
  if (text == null) return true;
  if (!context.mounted) return false;
  await showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      icon: const Icon(Icons.link_off),
      title: Text(context.l.linkFailed),
      content: Text(text),
      actions: [
        FilledButton(
          onPressed: () => Navigator.pop(context),
          child: Text(context.l.ok),
        ),
      ],
    ),
  );
  return false;
}

/// QR con su código debajo y si se puede llegar desde otras redes.
class LinkQr extends StatelessWidget {
  const LinkQr({super.key, required this.code, required this.what});

  final String? code;
  final String what;

  @override
  Widget build(BuildContext context) {
    final code = this.code;
    if (code == null) {
      return const Center(child: CircularProgressIndicator());
    }
    final anywhere = _reachableAnywhere(code);
    final scheme = Theme.of(context).colorScheme;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Center(
          child: Container(
            color: Colors.white,
            padding: const EdgeInsets.all(12),
            child: QrImageView(data: code, size: 220),
          ),
        ),
        const SizedBox(height: 8),
        Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Icon(
              anywhere ? Icons.public : Icons.wifi,
              size: 16,
              color: anywhere ? scheme.primary : scheme.outline,
            ),
            const SizedBox(width: 6),
            Flexible(
              child: Text(
                anywhere ? context.l.reachableAnywhere : context.l.reachableLocal,
                style: Theme.of(context).textTheme.bodySmall,
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        CodeBox(text: code, what: what),
      ],
    );
  }
}

/// Pide `load` ahora y cada [_refreshEvery] mientras el widget exista.
mixin _Refreshing<T extends StatefulWidget> on State<T> {
  String? code;
  Timer? _timer;

  Future<String> load();

  void startRefreshing() {
    Future<void> tick() async {
      try {
        final c = await load();
        if (mounted && c != code) setState(() => code = c);
      } catch (_) {
        // Se reintenta en el siguiente tick.
      }
    }

    tick();
    _timer = Timer.periodic(_refreshEvery, (_) => tick());
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }
}

/// Diálogo del primario: muestra la invitación y admite el código del
/// dispositivo nuevo (pegado o escaneado). Devuelve `true` si se vinculó.
class LinkDialog extends StatefulWidget {
  const LinkDialog({super.key, required this.store});

  final Store store;

  static Future<bool> show(BuildContext context, Store store) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (_) => LinkDialog(store: store),
    );
    return ok == true;
  }

  @override
  State<LinkDialog> createState() => _LinkDialogState();
}

class _LinkDialogState extends State<LinkDialog> with _Refreshing {
  final _code = TextEditingController();
  late final _label = TextEditingController(text: context.l.defaultDeviceLabel);
  StreamSubscription<String>? _joined;
  bool _busy = false;

  @override
  Future<String> load() => lacre.inviteDevice();

  @override
  void initState() {
    super.initState();
    startRefreshing();
    _joined = widget.store.deviceLinked.stream.listen((_) {
      if (mounted) Navigator.pop(context, true);
    });
  }

  @override
  void dispose() {
    _joined?.cancel();
    try {
      lacre.cancelInvite();
    } catch (_) {}
    super.dispose();
  }

  Future<void> _scan() async {
    final v = await scanQr(
      context,
      title: context.l.scanNewDeviceQr,
      accept: (v) => !v.startsWith('p2m:') && !lacre.isInvite(code: v),
    );
    if (v == null) return;
    _code.text = v;
    await _link();
  }

  Future<void> _link() async {
    if (_code.text.trim().isEmpty) return;
    setState(() => _busy = true);
    final r = await guard(
      context,
      () => lacre.linkDevice(code: _code.text, label: _label.text.trim()),
    );
    if (!mounted) return;
    setState(() => _busy = false);
    if (r != null && await explainLink(context, r) && mounted) {
      Navigator.pop(context, true);
    }
  }

  @override
  Widget build(BuildContext context) {
    final qr = [
      Text(canScan ? context.l.linkDialogOrQr : context.l.linkDialogInvite),
      const SizedBox(height: 12),
      LinkQr(code: code, what: context.l.invite),
      const SizedBox(height: 12),
      Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          const SizedBox.square(
            dimension: 16,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
          const SizedBox(width: 12),
          Flexible(child: Text(context.l.waitingNewDevice)),
        ],
      ),
    ];
    final paste = [
      Text(canScan ? context.l.linkDialogScan : context.l.linkDialogOrCode),
      const SizedBox(height: 12),
      if (canScan) ...[
        FilledButton.icon(
          onPressed: _busy ? null : _scan,
          icon: const Icon(Icons.qr_code_scanner),
          label: Text(context.l.scanNewDeviceQr),
        ),
        const SizedBox(height: 12),
      ],
      TextField(
        controller: _code,
        enabled: !_busy,
        decoration: InputDecoration(
          labelText: context.l.deviceCode,
          border: const OutlineInputBorder(),
        ),
        onSubmitted: (_) => _link(),
      ),
      const SizedBox(height: 12),
      TextField(
        controller: _label,
        enabled: !_busy,
        decoration: InputDecoration(
          labelText: context.l.deviceName,
          border: const OutlineInputBorder(),
        ),
      ),
    ];
    return AlertDialog(
      title: Text(context.l.linkDevice),
      content: SizedBox(
        width: 440,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: _busy
                ? [
                    const SizedBox(height: 16),
                    const Center(child: CircularProgressIndicator()),
                    const SizedBox(height: 16),
                    Text(context.l.linkConnecting, textAlign: TextAlign.center),
                  ]
                : [
                    // El móvil escanea; el escritorio muestra el QR.
                    ...(canScan ? paste : qr),
                    const Divider(height: 32),
                    ...(canScan ? qr : paste),
                  ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(context.l.cancel),
        ),
        if (!_busy)
          FilledButton(onPressed: _link, child: Text(context.l.link)),
      ],
    );
  }
}

/// Parte de la primera pantalla para vincular este dispositivo nuevo.
class JoinPanel extends StatefulWidget {
  const JoinPanel({super.key});

  @override
  State<JoinPanel> createState() => _JoinPanelState();
}

class _JoinPanelState extends State<JoinPanel> with _Refreshing {
  bool _busy = false;

  @override
  Future<String> load() => lacre.linkCode();

  @override
  void initState() {
    super.initState();
    startRefreshing();
  }

  Future<void> _join(String invite) async {
    setState(() => _busy = true);
    final r = await guard(
      context,
      () => lacre.joinIdentity(code: invite, label: newDeviceLabel(context)),
    );
    if (!mounted) return;
    setState(() => _busy = false);
    // Si sale bien, el evento `Linked` cambia de pantalla.
    // El aviso del evento `Linked` ya enseña el código de confirmación.
    if (r != null) await explainLink(context, r, showConfirm: false);
  }

  Future<void> _scan() async {
    final v = await scanQr(
      context,
      title: context.l.scanPrimaryQr,
      accept: (v) => lacre.isInvite(code: v),
    );
    if (v != null) await _join(v);
  }

  Future<void> _paste() async {
    final field = TextEditingController();
    final v = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.haveInvite),
        content: SizedBox(
          width: 420,
          child: TextField(
            controller: field,
            autofocus: true,
            decoration: InputDecoration(
              labelText: context.l.invite,
              border: const OutlineInputBorder(),
            ),
            onSubmitted: (v) => Navigator.pop(context, v),
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(context.l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, field.text),
            child: Text(context.l.link),
          ),
        ],
      ),
    );
    if (v != null && v.trim().isNotEmpty) await _join(v);
  }

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    if (_busy) {
      return Column(
        children: [
          const CircularProgressIndicator(),
          const SizedBox(height: 16),
          Text(context.l.linkConnecting, textAlign: TextAlign.center),
        ],
      );
    }
    final own = LinkQr(code: code, what: context.l.code);
    final waiting = Row(
      mainAxisAlignment: MainAxisAlignment.center,
      children: [
        const SizedBox.square(
          dimension: 16,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
        const SizedBox(width: 12),
        Flexible(child: Text(context.l.waitingPrimary)),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: canScan
          ? [
              Text(context.l.linkInstructionsScan, style: text.bodyMedium),
              const SizedBox(height: 16),
              FilledButton.icon(
                onPressed: _scan,
                icon: const Icon(Icons.qr_code_scanner),
                label: Text(context.l.scanPrimaryQr),
              ),
              const Divider(height: 40),
              Text(context.l.showOwnQr, style: text.bodyMedium),
              const SizedBox(height: 16),
              own,
              const SizedBox(height: 16),
              waiting,
            ]
          : [
              Text(context.l.linkInstructions, style: text.bodyMedium),
              const SizedBox(height: 16),
              own,
              const SizedBox(height: 16),
              waiting,
              const SizedBox(height: 8),
              TextButton.icon(
                onPressed: _paste,
                icon: const Icon(Icons.content_paste),
                label: Text(context.l.haveInvite),
              ),
            ],
    );
  }
}
