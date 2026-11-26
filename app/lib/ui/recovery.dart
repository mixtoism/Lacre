import 'dart:io';

import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/lacre.dart' as lacre;
import '../store.dart';
import 'common.dart';
import 'link.dart';

/// Frase de recuperación (D-048): verla, recuperar con ella y pasar el
/// papel de primario.

/// Enseña las 24 palabras numeradas, con el aviso de guardarlas bien.
Future<void> showRecoveryPhrase(BuildContext context) async {
  final words = await guard(context, () async => lacre.recoveryPhrase());
  if (words == null || !context.mounted) return;
  final list = words.split(' ');
  // En Android, sin capturas de pantalla mientras se ve.
  Future<void> secure(bool on) async {
    if (!Platform.isAndroid) return;
    try {
      await sistema.invokeMethod<bool>('pantallaSegura', on);
    } catch (_) {}
  }

  await secure(true);
  if (!context.mounted) return secure(false);
  await showDialog<void>(
    context: context,
    builder: (context) => AlertDialog(
      icon: const Icon(Icons.key),
      title: Text(context.l.recoveryPhrase),
      content: SingleChildScrollView(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(context.l.recoveryPhraseWarning),
            const SizedBox(height: 16),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (var i = 0; i < list.length; i++)
                  Chip(
                    label: Text(
                      '${i + 1}. ${list[i]}',
                      style: const TextStyle(fontFamily: 'monospace'),
                    ),
                  ),
              ],
            ),
          ],
        ),
      ),
      actions: [
        FilledButton(
          onPressed: () => Navigator.pop(context),
          child: Text(context.l.ok),
        ),
      ],
    ),
  );
  await secure(false);
}

/// Campo para escribir la frase y, si hace falta, quitar los demás
/// dispositivos. Llama a [onDone] con el handle al recuperar.
class RecoverPanel extends StatefulWidget {
  const RecoverPanel({
    super.key,
    required this.store,
    this.askOnlyThis = true,
    this.onDone,
  });

  final Store store;
  final bool askOnlyThis;
  final void Function(String handle)? onDone;

  @override
  State<RecoverPanel> createState() => _RecoverPanelState();
}

class _RecoverPanelState extends State<RecoverPanel> {
  final _phrase = TextEditingController();
  bool _onlyThis = false;
  bool _busy = false;

  @override
  void dispose() {
    _phrase.dispose();
    super.dispose();
  }

  Future<void> _recover() async {
    final phrase = _phrase.text.trim();
    if (phrase.isEmpty) return;
    setState(() => _busy = true);
    final handle = await guard(
      context,
      () => widget.store.recoverIdentity(
        phrase,
        newDeviceLabel(context),
        onlyThis: _onlyThis,
      ),
    );
    if (!mounted) return;
    setState(() => _busy = false);
    if (handle != null) {
      _phrase.clear();
      widget.onDone?.call(handle);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        TextField(
          controller: _phrase,
          enabled: !_busy,
          minLines: 3,
          maxLines: 5,
          autocorrect: false,
          enableSuggestions: false,
          keyboardType: TextInputType.visiblePassword,
          decoration: InputDecoration(
            labelText: context.l.recoveryPhrase,
            helperText: context.l.recoveryPhraseHelper,
            border: const OutlineInputBorder(),
          ),
        ),
        if (widget.askOnlyThis)
          CheckboxListTile(
            value: _onlyThis,
            onChanged: _busy ? null : (v) => setState(() => _onlyThis = v!),
            title: Text(context.l.recoverOnlyThis),
            contentPadding: EdgeInsets.zero,
            controlAffinity: ListTileControlAffinity.leading,
          ),
        const SizedBox(height: 16),
        FilledButton(
          onPressed: _busy ? null : _recover,
          child: _busy
              ? const SizedBox.square(
                  dimension: 20,
                  child: CircularProgressIndicator(strokeWidth: 2),
                )
              : Text(context.l.recoverIdentity),
        ),
      ],
    );
  }
}

/// Desde un dispositivo vinculado: pasa a ser primario con la frase.
Future<bool> becomePrimary(BuildContext context, Store store) async {
  final done = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      icon: const Icon(Icons.key),
      title: Text(context.l.becomePrimary),
      content: SingleChildScrollView(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Text(context.l.becomePrimaryBody),
            const SizedBox(height: 16),
            RecoverPanel(
              store: store,
              onDone: (_) => Navigator.pop(context, true),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(context.l.cancel),
        ),
      ],
    ),
  );
  return done == true;
}

/// Borra la clave raíz de aquí tras confirmarlo.
Future<bool> forgetRoot(BuildContext context, Store store) async {
  final ok = await showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      icon: const Icon(Icons.key_off),
      title: Text(context.l.forgetRoot),
      content: Text(context.l.forgetRootBody),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: Text(context.l.cancel),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, true),
          child: Text(context.l.forgetRootConfirm),
        ),
      ],
    ),
  );
  if (ok != true || !context.mounted) return false;
  final done = await guard(context, () async {
    store.forgetRoot();
    return true;
  });
  return done == true;
}
