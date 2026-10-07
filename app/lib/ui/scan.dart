import 'dart:io';

import 'package:flutter/material.dart';
import 'package:mobile_scanner/mobile_scanner.dart';

import '../l10n/l10n.dart';

/// Solo los móviles escanean; el escritorio muestra su QR y admite pegar.
bool get canScan => Platform.isAndroid || Platform.isIOS;

/// Abre la cámara y devuelve el primer QR que cumpla `accept`.
Future<String?> scanQr(
  BuildContext context, {
  required String title,
  bool Function(String)? accept,
}) {
  return Navigator.of(context).push<String>(
    MaterialPageRoute(
      fullscreenDialog: true,
      builder: (_) => _ScanPage(title: title, accept: accept ?? (_) => true),
    ),
  );
}

class _ScanPage extends StatefulWidget {
  const _ScanPage({required this.title, required this.accept});

  final String title;
  final bool Function(String) accept;

  @override
  State<_ScanPage> createState() => _ScanPageState();
}

class _ScanPageState extends State<_ScanPage> {
  bool _done = false;

  void _onDetect(BarcodeCapture capture) {
    if (_done) return;
    for (final b in capture.barcodes) {
      final v = b.rawValue?.trim();
      if (v != null && widget.accept(v)) {
        _done = true;
        Navigator.of(context).pop(v);
        return;
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(widget.title)),
      body: MobileScanner(
        onDetect: _onDetect,
        errorBuilder: (context, error) => Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text(
              error.errorCode == MobileScannerErrorCode.permissionDenied
                  ? context.l.noCameraPermission
                  : context.l.cantOpenCamera(error.errorCode.name),
              textAlign: TextAlign.center,
            ),
          ),
        ),
      ),
    );
  }
}
