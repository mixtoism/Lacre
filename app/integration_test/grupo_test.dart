// Prueba visible de un grupo entre el móvil y varias ventanas de la app de
// escritorio. La lanza `scripts/e2e-grupo.sh`, que compila este test como app
// de Linux, abre una ventana por usuario del PC y después lo ejecuta en el
// móvil.
//
// En el PC (LACRE_ROLE=ana, bea…): crea la identidad, deja su código en
// `$LACRE_E2E_DIR/<rol>.qr`, espera a que el móvil lo meta en el grupo, escribe
// «hola desde <rol>» y espera los mensajes de todos (`$LACRE_E2E_DIR/<rol>.ok`).
//
// En el móvil (ROLE=movil, PCS=códigos): crea la identidad, abre el grupo con
// los códigos de los PC, escribe «hola desde movil» y espera los de todos.
//
// Después, cuando todos han dicho «listo <rol>», el móvil (que creó el grupo)
// quita al último usuario del PC y escribe «sin <quitado>», que el quitado no
// debe recibir; luego lo vuelve a añadir y escribe «con <quitado>», y el
// quitado contesta «he vuelto».
//
// Por último, stickers y emojis (D-025): el móvil manda un emoji desde el
// panel y su sticker; cada PC guarda el del móvil en su colección y manda el
// suyo, y todos esperan los stickers de todos.
//
// Y archivos (D-026): el móvil manda uno de 1 MB (sin el selector de
// archivos, que no se puede manejar desde el test) y cada PC lo descarga con
// el botón «Descargar» y comprueba su contenido.
// Los PC reciben el grupo como solicitud de chat (D-029): lo aceptan desde
// el aviso del chat.
//
// Para terminar (D-029, D-030): cada uno puso al empezar su foto de perfil y
// espera las de los demás; busca dentro del chat (con anterior/siguiente),
// busca en todos los chats y abre un resultado, y borra el chat.
// Se coordina con el script por líneas `LACRE_E2E …`.
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:lacre/main.dart';
import 'package:lacre/src/rust/api/lacre.dart' as lacre;
import 'package:lacre/src/rust/frb_generated.dart';
import 'package:lacre/store.dart';
import 'package:lacre/ui/stickers.dart';
import 'package:path_provider/path_provider.dart';

const _roleDefine = String.fromEnvironment('ROLE', defaultValue: 'movil');
const _pcs = String.fromEnvironment('PCS');
const _users = String.fromEnvironment('USERS');
const _lan = bool.fromEnvironment('LAN');

/// Tor tarda: un circuito nuevo hasta un servicio onion lleva segundos, y un
/// servicio recién arrancado no es alcanzable hasta pasado un minuto o dos.
const paso = Duration(minutes: 10);

/// Sticker de prueba: un círculo de color con una letra.
Future<Uint8List> pegatina(String letra, Color color, {int lado = 256}) async {
  final rec = ui.PictureRecorder();
  final c = Canvas(rec);
  final centro = Offset(lado / 2, lado / 2);
  c.drawCircle(centro, lado * 0.45, Paint()..color = color);
  final tp = TextPainter(
    text: TextSpan(
      text: letra,
      style: TextStyle(fontSize: lado * 0.4, color: Colors.white),
    ),
    textDirection: TextDirection.ltr,
  )..layout();
  tp.paint(c, centro - Offset(tp.width / 2, tp.height / 2));
  final img = await rec.endRecording().toImage(lado, lado);
  final png = await img.toByteData(format: ui.ImageByteFormat.png);
  img.dispose();
  return png!.buffer.asUint8List();
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final env = Platform.environment;
  final role = env['LACRE_ROLE'] ?? _roleDefine;
  final shared = env['LACRE_E2E_DIR'];
  final pcsCodes = env['LACRE_PCS'] ?? _pcs;
  // Solo red local (mDNS), sin Tor: mucho más rápido.
  final lan = _lan || env['LACRE_LAN'] == '1';
  final users = (env['LACRE_USERS'] ?? _users)
      .split(' ')
      .where((u) => u.isNotEmpty);

  testWidgets('grupo móvil + PC: mensajes, quitar y añadir ($role)', (
    tester,
  ) async {
    await RustLib.init();
    void marca(String s) => debugPrint('LACRE_E2E $role $s');
    void fichero(String ext, String s) {
      if (shared != null) File('$shared/$role.$ext').writeAsStringSync(s);
    }

    /// Espera a que aparezca `f` (o, con `ausente`, a que desaparezca).
    Future<void> waitFor(
      Finder f, {
      Duration max = paso,
      bool ausente = false,
    }) async {
      final end = DateTime.now().add(max);
      while (f.evaluate().isEmpty != ausente) {
        if (DateTime.now().isAfter(end)) {
          fail(ausente ? 'no desapareció: $f' : 'no apareció: $f');
        }
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
      final campo = find.widgetWithText(TextField, 'Mensaje');
      // Los avisos se encolan: sin quitar los viejos, uno de error no se ve.
      ScaffoldMessenger.of(tester.element(campo)).clearSnackBars();
      await tester.pump();
      final ctl = tester.widget<TextField>(campo).controller!;
      // Al volver de otra pantalla el campo puede no recibir lo escrito (el
      // foco se restaura tarde): se reintenta hasta que el texto esté.
      for (var i = 0; i < 3 && ctl.text != text; i++) {
        if (i > 0) {
          marca(
            '«$text»: no se escribió (foco: '
            '${FocusManager.instance.primaryFocus}), reintento',
          );
          await tester.tap(campo);
          await tester.pump(const Duration(milliseconds: 200));
        }
        await tester.enterText(campo, text);
        await tester.pump(const Duration(milliseconds: 200));
      }
      if (ctl.text != text) {
        marca('«$text»: escrito directamente en el campo');
        ctl.text = text;
        await tester.pump();
      }
      // Como pulsar Intro: no depende de dónde quede el botón (el gestor de
      // ventanas puede dejarlas muy pequeñas).
      await tester.testTextInput.receiveAction(TextInputAction.send);
      await tester.pump();
      // Si la acción no llegó al campo (el foco quedó en otro sitio al
      // volver de otra pantalla), con el botón.
      if (ctl.text.isNotEmpty) {
        marca('«$text»: Intro no llegó, con el botón');
        await tester.ensureVisible(find.byTooltip('Enviar'));
        await tester.tap(find.byTooltip('Enviar'));
        await tester.pump();
      }
      await tester.pump(const Duration(milliseconds: 300));
      for (final e in find.byType(SnackBar).evaluate()) {
        final t = find.descendant(
          of: find.byWidget(e.widget),
          matching: find.byType(Text),
        );
        for (final w in t.evaluate()) {
          marca('aviso al enviar «$text»: ${(w.widget as Text).data}');
        }
      }
    }

    final base = shared ?? (await getTemporaryDirectory()).path;
    final dir = '$base/$role-${DateTime.now().millisecondsSinceEpoch}';
    final store = Store();
    await tester.pumpWidget(LacreApp(store: store));
    await store.start(dataDir: dir, lanOnly: lan);
    // Sin la pregunta de los avisos, que taparía la pantalla.
    await store.setNoticeContent(NoticeContent.full);
    await tester.pumpAndSettle();
    expect(store.startError, isNull);

    // Alta.
    await tester.enterText(find.byType(TextField), role);
    await tester.tap(find.text('Crear identidad'));
    await waitFor(find.byType(FloatingActionButton));
    marca('identidad ${store.profile!.handle}');
    final colores = [
      Colors.teal,
      Colors.deepOrange,
      Colors.indigo,
      Colors.pink,
    ];
    final color = colores[users.toList().indexOf(role) % colores.length];
    await esperar(
      store.setAvatar(
        await esperar(
          pegatina(role.characters.first.toUpperCase(), color, lado: 128),
        ),
      ),
    );
    await tester.pump();
    marca('foto de perfil puesta');

    if (role == 'movil') {
      // El móvil abre el grupo con los códigos de los PC.
      await tester.tap(find.byType(FloatingActionButton));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.widgetWithText(TextField, 'Participantes'),
        pcsCodes.split(' ').join('\n'),
      );
      await tester.enterText(
        find.widgetWithText(TextField, 'Título del grupo (opcional)'),
        'prueba',
      );
      // El primer circuito de Tor hasta un servicio onion puede no llegar
      // a tiempo: si falla, el diálogo sigue abierto y se reintenta.
      for (var intento = 1; ; intento++) {
        await tester.tap(find.text('Crear'));
        await tester.pump();
        await waitFor(find.byType(LinearProgressIndicator), ausente: true);
        await tester.pump(const Duration(seconds: 1));
        if (find.widgetWithText(TextField, 'Mensaje').evaluate().isNotEmpty) {
          break;
        }
        if (intento == 5) fail('no se pudo crear el grupo');
        marca('reintento $intento al crear el grupo');
      }
      await waitFor(find.widgetWithText(TextField, 'Mensaje'));
      marca('grupo creado');
    } else {
      // El PC publica su código (con el onion) y espera a que lo añadan.
      final qr = await esperar(() async {
        for (;;) {
          final q = await lacre.qrPayload();
          // En la red local se encuentran por mDNS: el código no
          // necesita direcciones.
          if (lan || q.contains('/onion3/')) return q;
          await Future<void>.delayed(const Duration(seconds: 5));
        }
      }());
      fichero('qr', qr);
      marca('qr listo');
      await waitFor(find.text('prueba'), max: const Duration(minutes: 30));
      expect(find.text('Solicitud'), findsOneWidget);
      await tester.tap(find.text('prueba'));
      await tester.pumpAndSettle();
      await waitFor(find.text('hola desde movil'));
      await waitFor(find.text('Te han invitado a «prueba»'));
      await tester.tap(find.widgetWithText(FilledButton, 'Aceptar'));
      await tester.pump();
      await waitFor(find.text('Te han invitado a «prueba»'), ausente: true);
      marca('en el grupo (solicitud aceptada)');
    }

    await enviar('hola desde $role');
    for (final u in users.where((u) => u != role)) {
      await waitFor(find.text('hola desde $u'));
      marca('recibido de $u');
    }

    // --- Quitar y volver a añadir a una persona.
    final pcs = users.where((u) => u != 'movil').toList();
    final quitado = pcs.last;
    Finder aviso(String s) => find.textContaining(s, findRichText: true);
    Future<void> menu(String item) async {
      await tester.tap(find.byTooltip('Más'));
      await tester.pumpAndSettle();
      await tester.tap(find.text(item));
      await tester.pumpAndSettle();
    }

    if (role == 'movil') {
      for (final u in pcs) {
        await waitFor(find.text('listo $u'));
      }
      final code = pcsCodes.split(' ').last;
      final handle = code.substring(4, code.indexOf(':', 4));
      await menu('Miembros');
      await tester.tap(find.byTooltip('Quitar a $handle'));
      await tester.pumpAndSettle();
      await tester.tap(find.widgetWithText(FilledButton, 'Quitar'));
      await tester.pump();
      await waitFor(find.byTooltip('Quitar a $handle'), ausente: true);
      await waitFor(find.byType(CircularProgressIndicator), ausente: true);
      marca('quitado $handle');
      await tester.pageBack();
      await tester.pumpAndSettle();
      await waitFor(aviso('quitó a $quitado#'));
      await enviar('sin $quitado');

      await menu('Miembros');
      await tester.tap(find.text('Añadir'));
      await tester.pumpAndSettle();
      await tester.enterText(find.widgetWithText(TextField, 'Personas'), code);
      await tester.tap(find.widgetWithText(FilledButton, 'Añadir'));
      await tester.pump();
      await waitFor(find.text('Añadir personas'), ausente: true);
      marca('añadido $handle');
      await tester.pageBack();
      await tester.pumpAndSettle();
      await waitFor(aviso('añadió a $quitado#'));
      await enviar('con $quitado');
      await waitFor(find.text('he vuelto'));
      marca('$quitado ha vuelto');
    } else {
      await enviar('listo $role');
      await waitFor(aviso('quitó a $quitado#'));
      if (role == quitado) {
        await waitFor(find.text('Ya no participas en este chat.'));
        marca('fuera del grupo');
        await waitFor(aviso('añadió a $quitado#'));
        await waitFor(find.widgetWithText(TextField, 'Mensaje'));
        await waitFor(find.text('con $quitado'));
        expect(find.text('sin $quitado'), findsNothing);
        await enviar('he vuelto');
        marca('de vuelta en el grupo');
      } else {
        await waitFor(find.text('sin $quitado'));
        await waitFor(find.text('con $quitado'));
        await waitFor(find.text('he vuelto'));
        marca('visto quitar y añadir a $quitado');
      }
    }

    // --- Emojis y stickers.
    final mio = lacre.addSticker(
      bytes: await esperar(
        pegatina(role.characters.first.toUpperCase(), color),
      ),
    );

    /// Remitentes de los stickers recibidos en el chat.
    Set<String> conSticker() => {
      for (final m in lacre.messages(index: 0))
        if (m.sticker != null) m.from.split('#').first,
    };
    Future<void> hasta(bool Function() ok) async {
      final end = DateTime.now().add(paso);
      while (!ok()) {
        if (DateTime.now().isAfter(end)) fail('tiempo agotado');
        await tester.pump(const Duration(milliseconds: 500));
      }
    }

    Future<void> enviarSticker(String id) async {
      await tester.tap(find.byTooltip('Emoji y stickers'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Stickers'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(ValueKey(id)));
      await tester.pump();
      await tester.tap(find.byTooltip('Emoji y stickers'));
      await tester.pumpAndSettle();
    }

    if (role == 'movil') {
      // Una imagen grande se reduce al importarla.
      final grande = await esperar(pegatina('M', Colors.teal, lado: 1000));
      final reducida = await esperar(fitSticker(grande));
      expect(reducida.length, lessThan(grande.length));
      // Emoji desde el panel.
      await tester.tap(find.byTooltip('Emoji y stickers'));
      await tester.pumpAndSettle();
      await tester.tap(find.byIcon(Icons.tag_faces));
      await tester.pumpAndSettle();
      await tester.tap(find.text('😀').first);
      await tester.pump();
      expect(
        tester.widget<TextField>(find.byType(TextField)).controller!.text,
        '😀',
      );
      // En el móvil la ventana es grande: el botón sí recibe el toque (la
      // acción «enviar» del teclado a veces no llegaba).
      await tester.tap(find.byTooltip('Emoji y stickers'));
      await tester.pumpAndSettle();
      await tester.tap(find.byTooltip('Enviar'));
      await hasta(
        () => tester
            .widget<TextField>(find.byType(TextField))
            .controller!
            .text
            .isEmpty,
      );
      marca('emoji enviado');
      await enviarSticker(mio.id);
      marca('sticker enviado');
    } else {
      await waitFor(find.text('😀'));
      await hasta(() => conSticker().contains('movil'));
      await tester.pump(const Duration(seconds: 1));
      final suyo = lacre
          .messages(index: 0)
          .lastWhere((m) => m.from.startsWith('movil#') && m.sticker != null)
          .sticker!;
      final imagen = find.byWidgetPredicate(
        (w) => w is StickerImage && w.path == suyo.path,
      );
      await waitFor(imagen);
      await tester.longPress(imagen);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Guardar en mis stickers'));
      await tester.pump();
      await waitFor(find.text('Guardado en tus stickers'));
      expect(lacre.stickers().map((s) => s.id), contains(suyo.id));
      marca('sticker del móvil guardado');
      await enviarSticker(mio.id);
      marca('sticker enviado');
    }
    for (final u in users.where((u) => u != role)) {
      await hasta(() => conSticker().contains(u));
      marca('sticker de $u');
    }

    // --- Archivos.
    final contenido = Uint8List.fromList(
      List.generate(1000000, (i) => (i * 7 + i ~/ 251) & 0xff),
    );
    if (role == 'movil') {
      final tmp = await esperar(getTemporaryDirectory());
      final f = File('${tmp.path}/de-movil.bin');
      f.writeAsBytesSync(contenido);
      await esperar(lacre.sendFile(index: 0, path: f.path));
      f.deleteSync();
      await waitFor(find.text('de-movil.bin'));
      await waitFor(find.byTooltip('Abrir'));
      marca('archivo enviado');
    } else {
      await waitFor(find.text('de-movil.bin'));
      await tester.tap(find.byTooltip('Descargar'));
      await tester.pump();
      await waitFor(find.byTooltip('Abrir'));
      final adjunto = lacre
          .messages(index: 0)
          .lastWhere((m) => m.file != null)
          .file!;
      expect(adjunto.path, isNotNull);
      expect(File(adjunto.path!).readAsBytesSync(), contenido);
      marca('archivo descargado');
    }

    // --- Fotos de perfil: llegan las de todos los del grupo.
    final otros = lacre
        .chatMembers(index: 0)
        .participants
        .where((h) => h != store.profile!.handle)
        .toList();
    for (final h in otros) {
      await hasta(() => lacre.avatarPath(handle: h) != null);
      marca('foto de ${h.split('#').first}');
    }
    await tester.pump(const Duration(seconds: 1));

    // --- Búsqueda dentro del chat: hay un «hola desde» de cada uno.
    await tester.tap(find.byTooltip('Buscar en el chat'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Buscar en este chat'),
      'HOLA DESDE',
    );
    await tester.pump();
    final n = users.length;
    await waitFor(find.text('1/$n'));
    await tester.pump(const Duration(seconds: 1));
    await tester.tap(find.byTooltip('Anterior (más antiguo)'));
    await tester.pump();
    await waitFor(find.text('2/$n'));
    await tester.pump(const Duration(seconds: 1));
    await tester.tap(find.byTooltip('Cerrar búsqueda'));
    await tester.pumpAndSettle();
    marca('búsqueda en el chat');

    // --- Búsqueda global: el mensaje de otro, y se abre en su chat.
    final otro = users.firstWhere((u) => u != role);
    await tester.pageBack();
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Buscar'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.widgetWithText(TextField, 'Buscar en todos los chats'),
      'desde $otro',
    );
    await tester.pump();
    await waitFor(find.text('1 mensaje'));
    await tester.pump(const Duration(seconds: 1));
    await tester.tap(find.text('prueba').last);
    await tester.pumpAndSettle();
    await waitFor(find.text('1/1'));
    marca('búsqueda global');
    await tester.pump(const Duration(seconds: 2));
    await tester.tap(find.byTooltip('Cerrar búsqueda'));
    await tester.pumpAndSettle();

    marca('ok');
    fichero('ok', '');
    // Los demás pueden seguir esperando nuestros mensajes un poco: se borra
    // al final, cuando ya han terminado todos.
    await hasta(
      () => users.every(
        (u) => shared == null || File('$shared/$u.ok').existsSync(),
      ),
    );
    await menu('Borrar chat');
    // El móvil gestiona el grupo y solo lo borra; los PCs salen de él.
    await tester.tap(
      find.widgetWithText(
        FilledButton,
        role == 'movil' ? 'Borrar' : 'Salir y borrar',
      ),
    );
    await tester.pumpAndSettle();
    // De vuelta en la lista, que aún tiene abierta la búsqueda global.
    await tester.tap(find.byTooltip('Cerrar búsqueda'));
    await tester.pumpAndSettle();
    await waitFor(find.text('Aún no tienes chats'));
    marca('chat borrado');
    // Deja la ventana abierta un rato para verla y para que los demás
    // terminen de recibir.
    await esperar(Future<void>.delayed(const Duration(seconds: 60)));
  }, timeout: const Timeout(Duration(minutes: 60)));
}
