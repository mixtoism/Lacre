import 'package:flutter/material.dart';

import 'src/rust/frb_generated.dart';
import 'store.dart';
import 'ui/chat_list.dart';
import 'ui/onboarding.dart';

Future<void> main() async {
  await RustLib.init();
  final store = Store();
  runApp(P2mApp(store: store));
  await store.start();
}

class P2mApp extends StatelessWidget {
  const P2mApp({super.key, required this.store});

  final Store store;

  @override
  Widget build(BuildContext context) {
    const seed = Color(0xFF2E7D6B);
    return MaterialApp(
      title: 'p2message',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(colorSchemeSeed: seed, useMaterial3: true),
      darkTheme: ThemeData(colorSchemeSeed: seed, brightness: Brightness.dark, useMaterial3: true),
      scaffoldMessengerKey: _messenger,
      home: _Notices(
        store: store,
        child: ListenableBuilder(
          listenable: store,
          builder: (context, _) {
            final profile = store.profile;
            if (profile == null) {
              return const Scaffold(body: Center(child: CircularProgressIndicator()));
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
