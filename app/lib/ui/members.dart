import 'package:flutter/material.dart';

import '../l10n/l10n.dart';
import '../src/rust/api/lacre.dart' as lacre;
import '../store.dart';
import 'common.dart';
import 'scan.dart';

/// Personas de un chat. Quien lo gestiona (D-007) puede añadir, quitar y
/// pasar la gestión a otra persona (D-049).
class MembersPage extends StatefulWidget {
  const MembersPage({super.key, required this.store, required this.index});

  final Store store;
  final int index;

  @override
  State<MembersPage> createState() => _MembersPageState();
}

class _MembersPageState extends State<MembersPage> {
  /// Persona que se está quitando (por Tor tarda).
  String? _removing;

  Future<void> _remove(String handle) async {
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.removeTitle(handle)),
        content: Text(context.l.removeBody),
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
    setState(() => _removing = handle);
    await guard(
      context,
      () => lacre.removeMember(index: widget.index, handle: handle),
    );
    if (mounted) setState(() => _removing = null);
  }

  Future<void> _handOver(String handle) async {
    final ok = await confirm(
      context,
      title: context.l.handOverTitle(handle),
      body: context.l.handOverBody,
      action: context.l.handOver,
    );
    if (!ok || !mounted) return;
    setState(() => _removing = handle);
    await guard(
      context,
      () => lacre.handOver(index: widget.index, handle: handle),
    );
    if (mounted) setState(() => _removing = null);
  }

  /// Número de seguridad (D-043): las dos partes deben ver el mismo.
  Future<void> _safety(String handle) async {
    final number = await guard(
      context,
      () async => lacre.safetyNumber(handle: handle),
    );
    if (number == null || !mounted) return;
    // 12 grupos de 5 cifras en 3 filas, más fáciles de comparar en voz alta.
    final groups = number.split(' ');
    final rows = [
      for (var i = 0; i < groups.length; i += 4)
        groups.skip(i).take(4).join(' '),
    ].join('\n');
    await showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(context.l.safetyTitle(handle)),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            CodeBox(text: rows, what: context.l.safetyNumber),
            const SizedBox(height: 16),
            Text(context.l.safetyBody),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.of(context).pop(),
            child: Text(context.l.close),
          ),
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: widget.store,
      builder: (context, _) {
        final members = lacre.chatMembers(index: widget.index);
        return Scaffold(
          appBar: AppBar(title: Text(context.l.members)),
          floatingActionButton: members.canEdit
              ? FloatingActionButton.extended(
                  onPressed: () => showDialog<void>(
                    context: context,
                    builder: (_) => AddMembersDialog(index: widget.index),
                  ),
                  icon: const Icon(Icons.person_add),
                  label: Text(context.l.add),
                )
              : null,
          body: ListView(
            children: [
              if (!members.canEdit)
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 4),
                  child: Text(context.l.onlyCreatorEdits),
                ),
              for (final p in members.participants)
                ListTile(
                  leading: Avatar(name: p, image: widget.store.avatar(p)),
                  title: Text(p.split('#').first),
                  subtitle: Text(p),
                  onTap: p == members.me ? null : () => _safety(p),
                  trailing: p == members.me
                      ? Text(context.l.you)
                      : _removing == p
                      ? const SizedBox.square(
                          dimension: 24,
                          child: CircularProgressIndicator(strokeWidth: 2),
                        )
                      : members.canEdit
                      ? PopupMenuButton<bool>(
                          enabled: _removing == null,
                          onSelected: (remove) =>
                              remove ? _remove(p) : _handOver(p),
                          itemBuilder: (context) => [
                            PopupMenuItem(
                              value: false,
                              child: ListTile(
                                leading: const Icon(Icons.admin_panel_settings),
                                title: Text(context.l.handOver),
                              ),
                            ),
                            PopupMenuItem(
                              value: true,
                              child: ListTile(
                                leading: const Icon(Icons.person_remove),
                                title: Text(context.l.removePerson(p)),
                              ),
                            ),
                          ],
                        )
                      : null,
                ),
            ],
          ),
        );
      },
    );
  }
}

class AddMembersDialog extends StatefulWidget {
  const AddMembersDialog({super.key, required this.index});

  final int index;

  @override
  State<AddMembersDialog> createState() => _AddMembersDialogState();
}

class _AddMembersDialogState extends State<AddMembersDialog> {
  final _targets = TextEditingController();
  bool _busy = false;

  Future<void> _scan() async {
    final code = await scanQr(
      context,
      title: context.l.scanProfileQr,
      accept: (v) => v.startsWith('p2m:'),
    );
    if (code == null || !mounted) return;
    final current = _targets.text.trim();
    setState(() => _targets.text = current.isEmpty ? code : '$current\n$code');
  }

  Future<void> _add() async {
    final targets = _targets.text
        .split(RegExp(r'[\s,]+'))
        .where((t) => t.isNotEmpty)
        .toList();
    if (targets.isEmpty) return;
    setState(() => _busy = true);
    final ok = await guard(context, () async {
      await lacre.addMembers(index: widget.index, targets: targets);
      return true;
    });
    if (!mounted) return;
    if (ok == true) {
      Navigator.of(context).pop();
    } else {
      setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(context.l.addPeople),
      content: SizedBox(
        width: 420,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            TextField(
              controller: _targets,
              autofocus: true,
              minLines: 2,
              maxLines: 5,
              enabled: !_busy,
              decoration: InputDecoration(
                labelText: context.l.people,
                helperText: context.l.peopleHelper,
                border: const OutlineInputBorder(),
                suffixIcon: canScan
                    ? IconButton(
                        tooltip: context.l.scanQr,
                        icon: const Icon(Icons.qr_code_scanner),
                        onPressed: _busy ? null : _scan,
                      )
                    : null,
              ),
            ),
            if (_busy) ...[
              const SizedBox(height: 16),
              const LinearProgressIndicator(),
              const SizedBox(height: 8),
              Text(context.l.searchingNetwork),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _busy ? null : () => Navigator.of(context).pop(),
          child: Text(context.l.cancel),
        ),
        FilledButton(
          onPressed: _busy ? null : _add,
          child: Text(context.l.add),
        ),
      ],
    );
  }
}
