import 'dart:convert';

import 'package:flutter/foundation.dart';

import 'src/rust/api/prefs.dart' as rust;

/// Merkzettel der Oberfläche (Ansicht, Sortierung, zuletzt gesucht …).
///
/// Die Werte liegen in der Rust-Datenbank. Diese Klasse hält eine Kopie im
/// Arbeitsspeicher, damit Widgets beim Bauen SOFORT einen Wert lesen können
/// (`get`), ohne auf die Datenbank zu warten. Geschrieben wird im
/// Hintergrund.
///
/// Verwendung:
///   final view = Prefs.instance.get('library.view.albums') ?? 'list';
///   Prefs.instance.set('library.view.albums', 'grid');
class Prefs extends ChangeNotifier {
  Prefs._();
  static final Prefs instance = Prefs._();

  final Map<String, String?> _cache = {};

  /// Schlüssel, die beim Start vorgeladen werden. Alles andere wird beim
  /// ersten Zugriff über [load] geholt.
  static final preload = <String>[
    'search.recent',
    'home.hint.swipe',
    'library.filter',
    'playback.crossfade',
    for (final f in [
      'playlists',
      'albums',
      'artists',
      'downloaded',
      'favorites',
      'smart',
    ]) ...['library.view.$f', 'library.sort.$f'],
  ];

  /// Lädt die vorab benötigten Werte. Einmal beim App-Start aufrufen.
  Future<void> init() async {
    for (final key in preload) {
      await load(key);
    }
  }

  /// Holt einen Wert aus der Datenbank in die Kopie.
  Future<String?> load(String key) async {
    try {
      final value = await rust.getPref(key: key);
      _cache[key] = value;
      return value;
    } catch (_) {
      // Ein nicht lesbarer Merkzettel ist kein Fehler für den Nutzer: Dann
      // gilt eben die Voreinstellung.
      return null;
    }
  }

  /// Wert aus der Kopie (sofort). `null`, wenn nie gesetzt.
  String? get(String key) => _cache[key];

  /// Setzt einen Wert; `null` löscht ihn.
  void set(String key, String? value) {
    if (_cache[key] == value) return;
    _cache[key] = value;
    notifyListeners();
    rust.setPref(key: key, value: value).catchError((_) {});
  }

  /// Liste von Texten (z. B. zuletzt gesuchte Begriffe), als JSON gespeichert.
  List<String> getList(String key) {
    final raw = get(key);
    if (raw == null) return const [];
    try {
      return (jsonDecode(raw) as List).cast<String>();
    } catch (_) {
      return const [];
    }
  }

  void setList(String key, List<String> values) => set(key, jsonEncode(values));
}
