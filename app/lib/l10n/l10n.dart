import 'dart:ui';

import 'package:flutter/widgets.dart';

import 'app_localizations.dart';

export 'app_localizations.dart';

/// Textos en el idioma de la app: `context.l.newChat`.
extension L10n on BuildContext {
  AppLocalizations get l => AppLocalizations.of(this);
}

/// Textos en el idioma del sistema, para donde no hay `BuildContext` (avisos
/// del almacén, errores de funciones sueltas). Es el mismo idioma que elige
/// la app.
AppLocalizations get systemL10n => lookupAppLocalizations(
  basicLocaleListResolution(
    PlatformDispatcher.instance.locales,
    AppLocalizations.supportedLocales,
  ),
);
