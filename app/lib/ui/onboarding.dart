import 'package:flutter/material.dart';
import 'package:qr_flutter/qr_flutter.dart';

import '../store.dart';
import 'common.dart';

/// Primera pantalla: crear identidad o vincular a una existente.
class OnboardingPage extends StatefulWidget {
  const OnboardingPage({super.key, required this.store});

  final Store store;

  @override
  State<OnboardingPage> createState() => _OnboardingPageState();
}

class _OnboardingPageState extends State<OnboardingPage> {
  final _name = TextEditingController();
  bool _busy = false;
  bool _linking = false;

  Future<void> _create() async {
    final name = _name.text.trim();
    if (name.isEmpty) return;
    setState(() => _busy = true);
    await guard(context, () => widget.store.createIdentity(name));
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    final text = Theme.of(context).textTheme;
    final code = widget.store.profile!.deviceCode;
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(24),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [
              Icon(Icons.forum_outlined, size: 64, color: Theme.of(context).colorScheme.primary),
              const SizedBox(height: 16),
              Text('p2message', style: text.headlineMedium, textAlign: TextAlign.center),
              const SizedBox(height: 8),
              Text(
                'Mensajería cifrada de extremo a extremo, sin servidores. '
                'Tus claves no salen de tus dispositivos.',
                style: text.bodyMedium,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 32),
              SegmentedButton<bool>(
                segments: const [
                  ButtonSegment(value: false, label: Text('Identidad nueva')),
                  ButtonSegment(value: true, label: Text('Vincular dispositivo')),
                ],
                selected: {_linking},
                onSelectionChanged: (s) => setState(() => _linking = s.first),
              ),
              const SizedBox(height: 24),
              if (!_linking) ...[
                TextField(
                  controller: _name,
                  autofocus: true,
                  enabled: !_busy,
                  decoration: const InputDecoration(
                    labelText: 'Nombre',
                    helperText: 'Minúsculas, números y _. Se le añadirá un sufijo: nombre#xxxxxxxx',
                    border: OutlineInputBorder(),
                  ),
                  onSubmitted: (_) => _create(),
                ),
                const SizedBox(height: 16),
                FilledButton(
                  onPressed: _busy ? null : _create,
                  child: _busy
                      ? const SizedBox.square(dimension: 20, child: CircularProgressIndicator(strokeWidth: 2))
                      : const Text('Crear identidad'),
                ),
              ] else ...[
                Text(
                  'En tu dispositivo principal abre Perfil → Dispositivos → Vincular '
                  'y pega este código. Esta pantalla cambiará sola cuando se complete.',
                  style: text.bodyMedium,
                ),
                const SizedBox(height: 16),
                Center(
                  child: Container(
                    color: Colors.white,
                    padding: const EdgeInsets.all(12),
                    child: QrImageView(data: code, size: 200),
                  ),
                ),
                const SizedBox(height: 16),
                CodeBox(text: code, what: 'Código'),
                const SizedBox(height: 16),
                const Row(mainAxisAlignment: MainAxisAlignment.center, children: [
                  SizedBox.square(dimension: 16, child: CircularProgressIndicator(strokeWidth: 2)),
                  SizedBox(width: 12),
                  Text('Esperando al dispositivo principal…'),
                ]),
              ],
            ]),
          ),
        ),
      ),
    );
  }
}
