import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';

import '../../src/rust/api/library.dart' as rust;

/// Zeigt ein Album-Cover.
///
/// Drei Dinge sind hier wichtig für Akku und Datenvolumen:
///  1. Wir fordern vom Server nur die Größe an, die wirklich angezeigt wird
///     (mal Pixeldichte des Displays), nicht das Original.
///  2. `CachedNetworkImage` speichert das Bild auf der Platte. Beim nächsten
///     Öffnen wird nichts erneut heruntergeladen.
///  3. `memCacheWidth` sorgt dafür, dass das Bild auch im RAM nur in
///     Anzeigegröße liegt.
class Cover extends StatelessWidget {
  const Cover({
    super.key,
    required this.coverId,
    required this.size,
    this.radius = 4,
  });

  final String? coverId;

  /// Kantenlänge in logischen Pixeln.
  final double size;
  final double radius;

  /// Rundet auf feste Stufen. Sonst würde jede leicht andere Anzeigegröße
  /// eine eigene Datei im Cache erzeugen.
  static int bucket(double physicalPx) {
    const steps = [64, 128, 256, 512, 1024];
    return steps.firstWhere((s) => s >= physicalPx, orElse: () => steps.last);
  }

  @override
  Widget build(BuildContext context) {
    final placeholder = Container(
      width: size,
      height: size,
      color: Theme.of(context).colorScheme.surfaceContainer,
      child: Icon(
        Icons.music_note,
        size: size * 0.45,
        color: Theme.of(context).hintColor,
      ),
    );

    final id = coverId;
    final px = bucket(size * MediaQuery.devicePixelRatioOf(context));
    final url = id == null ? null : rust.coverUrl(coverId: id, size: px);

    // Solange das große Bild lädt, zeigen wir die kleine Fassung, die
    // meist schon aus einer Liste im Cache liegt. So erscheint im Player
    // sofort das Cover (unscharf), statt erst ein leerer Kasten.
    Widget loading(BuildContext context) {
      const small = 128;
      if (id == null || px <= small) return placeholder;
      final smallUrl = rust.coverUrl(coverId: id, size: small);
      if (smallUrl == null) return placeholder;
      return CachedNetworkImage(
        imageUrl: smallUrl,
        cacheKey: 'cover-$id-$small',
        width: size,
        height: size,
        fit: BoxFit.cover,
        fadeInDuration: Duration.zero,
        placeholder: (_, _) => placeholder,
        errorWidget: (_, _, _) => placeholder,
      );
    }

    return ClipRRect(
      borderRadius: BorderRadius.circular(radius),
      child: url == null
          ? placeholder
          : CachedNetworkImage(
              imageUrl: url,
              // Eigener Cache-Schlüssel OHNE Anmeldedaten: Die URL enthält
              // Token und Salt. Nach einer neuen Anmeldung ändern sich die,
              // das Bild ist aber dasselbe und soll nicht neu geladen werden.
              cacheKey: 'cover-$id-$px',
              width: size,
              height: size,
              memCacheWidth: px,
              fit: BoxFit.cover,
              fadeInDuration: const Duration(milliseconds: 120),
              placeholder: (context, _) => loading(context),
              errorWidget: (context, _, _) => loading(context),
            ),
    );
  }
}
