import 'package:cached_network_image/cached_network_image.dart';
import 'package:flutter/material.dart';

import '../src/rust/api/library.dart' as rust;
import '../src/rust/api/settings.dart';

/// Eigene Akzentfarbe der App (bewusst NICHT das Spotify-Grün – Aufbau und
/// Bedienung sind wie bei Spotify, die Marke ist eine eigene).
const kAccent = Color(0xFF8B7CFF);

/// Die Akzentfarben, die in den Einstellungen zur Auswahl stehen.
///
/// Feste Palette statt freier Farbwahl: Jede dieser Farben ist hell genug,
/// um auf dem dunklen Hintergrund gut lesbar zu sein, und kräftig genug für
/// den hellen. Bei einem freien Farbwähler könnte man leicht eine Farbe
/// erwischen, mit der Knöpfe und markierte Titel kaum noch zu erkennen sind.
const kAccentChoices = <Color>[
  Color(0xFF8B7CFF), // Violett (Standard, wie das App-Icon)
  Color(0xFF4C9BFF), // Blau
  Color(0xFF2CC5C5), // Türkis
  Color(0xFF3DDC84), // Grün
  Color(0xFFFFC043), // Bernstein
  Color(0xFFFF8A3D), // Orange
  Color(0xFFFF5C7A), // Rot/Pink
  Color(0xFFE879F9), // Magenta
];

/// Baut die Themes für Dunkel, Echtes Schwarz und Hell.
class AppTheme {
  /// Dunkles Grau wie bei Spotify.
  static ThemeData dark([Color accent = kAccent]) => _build(
    accent: accent,
    brightness: Brightness.dark,
    background: const Color(0xFF121212),
    surface: const Color(0xFF1E1E1E),
  );

  /// Reines Schwarz: Auf OLED-Displays sind schwarze Pixel wirklich aus.
  /// Das spart messbar Akku, weil die App viel Fläche in der Hintergrundfarbe hat.
  static ThemeData black([Color accent = kAccent]) => _build(
    accent: accent,
    brightness: Brightness.dark,
    background: Colors.black,
    surface: const Color(0xFF101010),
  );

  static ThemeData light([Color accent = kAccent]) => _build(
    accent: accent,
    brightness: Brightness.light,
    background: const Color(0xFFFAFAFA),
    surface: Colors.white,
  );

  static ThemeData _build({
    required Color accent,
    required Brightness brightness,
    required Color background,
    required Color surface,
  }) {
    final scheme = ColorScheme.fromSeed(
      seedColor: accent,
      brightness: brightness,
    ).copyWith(surface: background, surfaceContainer: surface, primary: accent);
    return ThemeData(
      useMaterial3: true,
      colorScheme: scheme,
      scaffoldBackgroundColor: background,
      appBarTheme: AppBarTheme(
        backgroundColor: background,
        surfaceTintColor: Colors.transparent,
        elevation: 0,
      ),
      navigationBarTheme: NavigationBarThemeData(
        backgroundColor: background,
        surfaceTintColor: Colors.transparent,
        indicatorColor: accent.withValues(alpha: 0.25),
        height: 64,
      ),
      // Keine Wellen-Animation beim Tippen: wirkt ruhiger (wie Spotify) und
      // spart ein paar Frames Rechenarbeit pro Tipp.
      splashFactory: NoSplash.splashFactory,
    );
  }

  /// Übersetzt die Einstellung in das, was `MaterialApp` braucht.
  ///
  /// Bei "System" entscheidet Android zwischen hell und dunkel; welches
  /// Dunkel dann gilt (normal oder echtes Schwarz), kommt aus
  /// `systemDarkVariant` – so wie in der Spezifikation gewünscht.
  static ({ThemeMode mode, ThemeData light, ThemeData dark}) resolve(
    AppSettings s,
  ) {
    final accent = Color(s.accentColor);
    final systemDark = s.systemDarkVariant == DarkVariant.black
        ? black(accent)
        : dark(accent);
    return switch (s.theme) {
      ThemeChoice.dark => (
        mode: ThemeMode.dark,
        light: light(accent),
        dark: dark(accent),
      ),
      ThemeChoice.black => (
        mode: ThemeMode.dark,
        light: light(accent),
        dark: black(accent),
      ),
      ThemeChoice.light => (
        mode: ThemeMode.light,
        light: light(accent),
        dark: systemDark,
      ),
      ThemeChoice.system => (
        mode: ThemeMode.system,
        light: light(accent),
        dark: systemDark,
      ),
    };
  }
}

/// Liefert die Hauptfarbe eines Covers, damit sich Player- und Album-Seite
/// wie bei Spotify passend einfärben.
class CoverColors {
  /// Einmal berechnete Farben merken: Die Berechnung muss das Bild
  /// dekodieren, das soll nicht bei jedem Öffnen derselben Seite passieren.
  static final Map<String, Color> _cache = {};

  static Future<Color?> of(String? coverId) async {
    if (coverId == null) return null;
    final cached = _cache[coverId];
    if (cached != null) return cached;
    // 64 px reichen für die Farbbestimmung völlig aus und sind schnell
    // geladen und ausgewertet (weniger Daten, weniger CPU).
    final url = rust.coverUrl(coverId: coverId, size: 64);
    if (url == null) return null;
    try {
      final scheme = await ColorScheme.fromImageProvider(
        provider: CachedNetworkImageProvider(
          url,
          cacheKey: 'cover-$coverId-64',
        ),
        brightness: Brightness.dark,
      );
      // `primaryContainer` ist kräftig, aber dunkel genug, dass weiße
      // Schrift darauf lesbar bleibt.
      return _cache[coverId] = scheme.primaryContainer;
    } catch (_) {
      // Kein Cover oder kein Netz: dann eben ohne Einfärbung.
      return null;
    }
  }
}
