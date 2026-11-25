import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../store.dart';
import 'common.dart';
import 'link.dart';

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
    return Scaffold(
      body: Center(
        child: SingleChildScrollView(
          padding: const EdgeInsets.all(24),
          child: ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 420),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Icon(
                  Icons.forum_outlined,
                  size: 64,
                  color: Theme.of(context).colorScheme.primary,
                ),
                const SizedBox(height: 16),
                Text(
                  'Lacre',
                  style: text.headlineMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 8),
                Text(
                  context.l.onboardingTagline,
                  style: text.bodyMedium,
                  textAlign: TextAlign.center,
                ),
                const SizedBox(height: 32),
                SegmentedButton<bool>(
                  segments: [
                    ButtonSegment(
                      value: false,
                      label: Text(context.l.newIdentity),
                    ),
                    ButtonSegment(
                      value: true,
                      label: Text(context.l.linkDevice),
                    ),
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
                    decoration: InputDecoration(
                      labelText: context.l.name,
                      helperText: context.l.nameHelper,
                      border: const OutlineInputBorder(),
                    ),
                    onSubmitted: (_) => _create(),
                  ),
                  const SizedBox(height: 16),
                  FilledButton(
                    onPressed: _busy ? null : _create,
                    child: _busy
                        ? const SizedBox.square(
                            dimension: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : Text(context.l.createIdentity),
                  ),
                ] else
                  const JoinPanel(),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
