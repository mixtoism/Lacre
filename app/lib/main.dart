import 'package:flutter/material.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart';

import 'l10n/l10n.dart';

import 'src/rust/frb_generated.dart';
import 'store.dart';
import 'ui/chat_list.dart';
import 'ui/common.dart';
import 'ui/onboarding.dart';

Future<void> main() async {
  await RustLib.init();
  final store = Store();
  runApp(LacreApp(store: store));
  await store.start();
}

class LacreApp extends StatelessWidget {
  const LacreApp({super.key, required this.store});

  final Store store;

  @override
  Widget build(BuildContext context) {
    // Rojo lacre, el del sello del icono. Fijo como `primary` en los dos
    // modos: el texto blanco encima cumple el contraste AA (7,2:1) y se sigue
    // viendo sobre fondo oscuro.
    const brand = Color(0xFFA8231F);
    ColorScheme scheme(Brightness b) => ColorScheme.fromSeed(
      seedColor: brand,
      brightness: b,
      dynamicSchemeVariant: DynamicSchemeVariant.fidelity,
    ).copyWith(primary: brand, onPrimary: Colors.white);
    return MaterialApp(
      title: 'Lacre',
      debugShowCheckedModeBanner: false,
      // Sin `locale`: Flutter sigue el idioma del sistema; si no está
      // traducido, el primero de la lista (inglés, ver l10n.yaml).
      localizationsDelegates: const [
        AppLocalizations.delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
      ],
      supportedLocales: AppLocalizations.supportedLocales,
      // Para los formatos de `intl` (fechas y tamaños, ver common.dart).
      builder: (context, child) {
        Intl.defaultLocale = Localizations.localeOf(context).toLanguageTag();
        return child!;
      },
      theme: ThemeData(
        colorScheme: scheme(Brightness.light),
        useMaterial3: true,
        fontFamilyFallback: fontFallback,
      ),
      darkTheme: ThemeData(
        colorScheme: scheme(Brightness.dark),
        fontFamilyFallback: fontFallback,
        useMaterial3: true,
      ),
      scaffoldMessengerKey: _messenger,
      home: _Notices(
        store: store,
        child: ListenableBuilder(
          listenable: store,
          builder: (context, _) {
            final profile = store.profile;
            if (store.startError != null) {
              return Scaffold(
                body: Center(
                  child: Padding(
                    padding: const EdgeInsets.all(24),
                    child: Text(
                      context.l.startError('${store.startError}'),
                      textAlign: TextAlign.center,
                    ),
                  ),
                ),
              );
            }
            if (profile == null) {
              return const Scaffold(
                body: Center(child: CircularProgressIndicator()),
              );
            }
            if (profile.handle == null) return OnboardingPage(store: store);
            return ChatListPage(store: store);
          },
        ),
      ),
    );
  }
}

final _messenger = GlobalKey<ScaffoldMessengerState>();

/// Muestra los avisos del nodo como SnackBar.
class _Notices extends StatefulWidget {
  const _Notices({required this.store, required this.child});

  final Store store;
  final Widget child;

  @override
  State<_Notices> createState() => _NoticesState();
}

class _NoticesState extends State<_Notices> {
  @override
  void initState() {
    super.initState();
    widget.store.notices.stream.listen((text) {
      _messenger.currentState?.showSnackBar(SnackBar(content: Text(text)));
    });
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
