// Alta en la app y conversación con un nodo de la CLI en el mismo equipo
// (se encuentran por mDNS). Requiere `cargo build -p p2m-node`.
//
// Todo va en un solo test: la app tiene un único nodo por proceso.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:p2message/main.dart';
import 'package:p2message/src/rust/frb_generated.dart';
import 'package:p2message/store.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(() async => await RustLib.init());

  testWidgets('alta y chat con un nodo de la CLI', (tester) async {
    final tmp = await Directory.systemTemp.createTemp('p2m-interop');
    final bin = '${Directory.current.parent.path}/target/debug/p2m-node';
    final cli = await Process.start(bin, [
      '--data',
      '${tmp.path}/cli',
      '--tick',
      '5',
      '--no-tor',
      '--no-rendezvous',
    ]);
    final out = StringBuffer();
    cli.stdout.transform(utf8.decoder).listen(out.write);
    addTearDown(cli.kill);

    Future<String> waitOut(RegExp re) async {
      for (var i = 0; i < 300; i++) {
        final m = re.firstMatch(out.toString());
        if (m != null) return m.groupCount > 0 ? m.group(1)! : m.group(0)!;
        await tester.pump(const Duration(milliseconds: 200));
      }
      fail('la CLI no mostró $re:\n$out');
    }

    Future<void> waitFor(Finder f) async {
      for (var i = 0; i < 450 && f.evaluate().isEmpty; i++) {
        await tester.pump(const Duration(milliseconds: 200));
      }
      expect(f, findsWidgets);
    }

    cli.stdin.writeln('init bob');
    final bob = await waitOut(RegExp(r'identidad creada: (\S+)'));

    final store = Store();
    await tester.pumpWidget(P2mApp(store: store));
    await store.start(dataDir: '${tmp.path}/app');
    // Sin la pregunta de los avisos, que taparía la pantalla.
    await store.setNoticeContent(NoticeContent.full);
    await tester.pumpAndSettle();
    expect(find.text('Crear identidad'), findsOneWidget);
    await tester.enterText(find.byType(TextField), 'ana');
    await tester.tap(find.text('Crear identidad'));
    await waitFor(find.byType(FloatingActionButton));
    expect(find.textContaining('ana#'), findsOneWidget);
    // Esperar a ver a la CLI por mDNS.
    for (var i = 0; i < 150 && (store.status?.peers ?? 0) == 0; i++) {
      await tester.pump(const Duration(milliseconds: 200));
    }

    await tester.tap(find.byType(FloatingActionButton));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Participantes'),
      bob,
    );
    await tester.tap(find.text('Crear'));
    await waitFor(find.widgetWithText(TextField, 'Mensaje'));

    await tester.enterText(
      find.widgetWithText(TextField, 'Mensaje'),
      'hola desde la app',
    );
    await tester.tap(find.byTooltip('Enviar'));
    await waitOut(RegExp(r'ana#\S+ .*: hola desde la app'));
    await waitFor(find.text('hola desde la app'));

    await waitOut(RegExp(r'nuevo chat \[0\]'));
    cli.stdin.writeln('send 0 hola desde la cli');
    await waitFor(find.text('hola desde la cli'));

    // Con el otro extremo desconectado, el mensaje aparece al momento y se
    // puede seguir escribiendo; la entrega sigue en segundo plano.
    cli.kill();
    await cli.exitCode;
    final input = find.widgetWithText(TextField, 'Mensaje');
    await tester.enterText(input, 'sin conexión');
    await tester.tap(find.byTooltip('Enviar'));
    for (
      var i = 0;
      i < 10 && find.text('sin conexión').evaluate().isEmpty;
      i++
    ) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(
      find.descendant(
        of: find.byType(ListView),
        matching: find.text('sin conexión'),
      ),
      findsOneWidget,
    );
    expect(tester.widget<TextField>(input).controller!.text, isEmpty);
  });
}
