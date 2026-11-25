// Prueba completa entre el móvil y el PC. La lanza `scripts/e2e-movil.sh`,
// que arranca en el PC los nodos A y B (con identidad) y C (sin identidad,
// para vincularlo al móvil) y contesta a los «ping» que reciben.
//
// El móvil y el PC no tienen por qué estar en la misma red: los códigos
// `p2m:` llevan la dirección onion de cada nodo (D-020, D-021).
//
// Coordinación: el móvil escribe líneas `LACRE_E2E …` en la salida del test,
// y el script reacciona a ellas.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:lacre/main.dart';
import 'package:lacre/src/rust/api/lacre.dart' as lacre;
import 'package:lacre/src/rust/frb_generated.dart';
import 'package:lacre/store.dart';
import 'package:path_provider/path_provider.dart';

const pcA = String.fromEnvironment('PC_A');
const pcB = String.fromEnvironment('PC_B');
const pcCLink = String.fromEnvironment('PC_C_LINK');

/// Tor tarda: un circuito nuevo hasta un servicio onion lleva segundos, y un
/// servicio recién arrancado no es alcanzable hasta pasado un minuto o dos.
const paso = Duration(minutes: 6);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'móvil ↔ PC: chats, grupo, buzones y vinculación',
    (tester) async {
      await RustLib.init();
      void marca(String s) => debugPrint('LACRE_E2E $s');

      Future<void> waitFor(Finder f, {Duration max = paso}) async {
        final end = DateTime.now().add(max);
        while (f.evaluate().isEmpty) {
          if (DateTime.now().isAfter(end)) fail('no apareció: $f');
          await tester.pump(const Duration(milliseconds: 500));
        }
      }

      Future<T> esperar<T>(Future<T> f) async {
        T? r;
        Object? err;
        var done = false;
        f
            .then((v) => r = v, onError: (e) => err = e)
            .whenComplete(() => done = true);
        final end = DateTime.now().add(paso);
        while (!done) {
          if (DateTime.now().isAfter(end)) fail('tiempo agotado');
          await tester.pump(const Duration(milliseconds: 500));
        }
        if (err != null) throw err!;
        return r as T;
      }

      Future<void> enviar(String text) async {
        final input = find.widgetWithText(TextField, 'Mensaje');
        await tester.enterText(input, text);
        await tester.tap(find.byTooltip('Enviar'));
        await tester.pump();
      }

      Future<void> nuevoChat(String participantes, {String? titulo}) async {
        await tester.tap(find.byType(FloatingActionButton));
        await tester.pumpAndSettle();
        await tester.enterText(
          find.widgetWithText(TextField, 'Participantes'),
          participantes,
        );
        if (titulo != null) {
          await tester.enterText(
            find.widgetWithText(TextField, 'Título del grupo (opcional)'),
            titulo,
          );
        }
        await tester.tap(find.text('Crear'));
        await waitFor(find.widgetWithText(TextField, 'Mensaje'));
      }

      // Directorio nuevo: no toca la identidad de la app instalada.
      final tmp = await getTemporaryDirectory();
      final dir = '${tmp.path}/e2e-${DateTime.now().millisecondsSinceEpoch}';
      final store = Store();
      await tester.pumpWidget(LacreApp(store: store));
      await store.start(dataDir: dir);
      // Sin la pregunta de los avisos, que taparía la pantalla.
      await store.setNoticeContent(NoticeContent.full);
      await tester.pumpAndSettle();
      expect(store.startError, isNull);

      // 1. Alta.
      await tester.enterText(find.byType(TextField), 'movil');
      await tester.tap(find.text('Crear identidad'));
      await waitFor(find.byType(FloatingActionButton));
      marca('identidad ${store.profile!.handle}');

      // 2. Chat 1:1 iniciado desde el móvil, en los dos sentidos.
      await nuevoChat(pcA);
      await enviar('ping 1');
      await waitFor(find.text('pong ping 1 desde A'));
      marca('ok 1:1 movil→A→movil');

      // 3. Grupo móvil + A + B.
      await tester.pageBack();
      await tester.pumpAndSettle();
      await nuevoChat('$pcA $pcB', titulo: 'prueba');
      await enviar('ping 2');
      await waitFor(find.text('pong ping 2 desde A'));
      await waitFor(find.text('pong ping 2 desde B'));
      marca('ok grupo');

      // 4. Chat iniciado desde el PC con nuestro código (lleva el onion).
      final qr = await esperar(() async {
        // El onion tarda en publicarse; hasta entonces el código no lo lleva.
        for (;;) {
          final q = await lacre.qrPayload();
          if (q.contains('/onion3/')) return q;
          await Future<void>.delayed(const Duration(seconds: 5));
        }
      }());
      marca('qr=$qr');
      await tester.pageBack();
      await tester.pumpAndSettle();
      await waitFor(find.textContaining('hola movil desde B'));
      marca('ok chat desde B');

      // 5. Buzones: A se desconecta, le escribimos en el grupo y lo recibe
      //    al volver.
      await tester.tap(find.text('prueba'));
      await tester.pumpAndSettle();
      marca('apaga A');
      await esperar(Future<void>.delayed(const Duration(seconds: 15)));
      await enviar('ping 3');
      await esperar(Future<void>.delayed(const Duration(seconds: 20)));
      marca('enciende A');
      await waitFor(
        find.text('pong ping 3 desde A'),
        max: const Duration(minutes: 10),
      );
      marca('ok buzones');

      // 6. Vincular C como segundo dispositivo del móvil. El script espera a
      //    que C entre en el grupo y hace que B escriba en él; tienen que
      //    recibirlo el móvil y C.
      expect(
        await esperar(lacre.linkDevice(code: pcCLink, label: 'pc')),
        isA<lacre.LinkResult_Done>(),
      );
      expect(lacre.devices().length, 2);
      marca('vinculado');
      await waitFor(
        find.text('pong vinculado desde B'),
        max: const Duration(minutes: 10),
      );
      marca('ok vinculación');
    },
    skip: pcA.isEmpty,
    timeout: const Timeout(Duration(minutes: 60)),
  );
}
