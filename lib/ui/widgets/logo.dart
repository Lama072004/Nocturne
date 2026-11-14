import 'package:flutter/material.dart';

/// Das Nocturne-Logo: fünf abgerundete Klangbalken.
///
/// Gezeichnet statt als Bilddatei eingebunden: Es bleibt in jeder Größe
/// scharf, lässt sich einfärben und braucht keine Datei in der App.
/// Die Proportionen sind dieselben wie im App-Icon
/// (`android/.../drawable/ic_launcher_foreground.xml`), damit beides gleich
/// aussieht.
class NocturneLogo extends StatelessWidget {
  const NocturneLogo({super.key, this.size = 64, this.color});

  final double size;

  /// Standard: Akzentfarbe des Themes.
  final Color? color;

  @override
  Widget build(BuildContext context) => CustomPaint(
    size: Size.square(size),
    painter: _LogoPainter(color ?? Theme.of(context).colorScheme.primary),
  );
}

class _LogoPainter extends CustomPainter {
  _LogoPainter(this.color);

  final Color color;

  /// Balkenhöhen im Verhältnis zur höchsten (wie im App-Icon: 14/30/44/30/14).
  static const _heights = [14 / 44, 30 / 44, 1.0, 30 / 44, 14 / 44];

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()..color = color;
    // Im Icon: 5 Balken à 6 Einheiten + 4 Lücken à 4,5 = 48 Einheiten Breite.
    final unit = size.width / 48;
    final barWidth = 6 * unit;
    final gap = 4.5 * unit;
    final maxHeight = size.height * 44 / 48;

    for (var i = 0; i < _heights.length; i++) {
      final h = maxHeight * _heights[i];
      final rect = Rect.fromLTWH(
        i * (barWidth + gap),
        (size.height - h) / 2,
        barWidth,
        h,
      );
      // Radius = halbe Breite ergibt die halbrunden Enden.
      canvas.drawRRect(
        RRect.fromRectAndRadius(rect, Radius.circular(barWidth / 2)),
        paint,
      );
    }
  }

  @override
  bool shouldRepaint(_LogoPainter old) => old.color != color;
}
