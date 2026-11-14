import 'package:flutter/foundation.dart';

import '../src/rust/api/library.dart' as rust;
import '../src/rust/api/models.dart';

/// Merkt sich Favoriten-Änderungen, die in dieser Sitzung gemacht wurden.
///
/// WARUM nötig? Ein `Track` vom Server ist unveränderlich und trägt den
/// Favoriten-Status vom Zeitpunkt des Ladens. Markiert der Nutzer einen
/// Titel, müssten sonst alle offenen Listen neu vom Server geladen werden,
/// nur damit das Herz richtig aussieht. Stattdessen merken wir uns hier die
/// Änderung und fragen überall über [isStarred] ab.
class FavoriteStore extends ChangeNotifier {
  /// Schlüssel: "art:id", Wert: aktueller Status.
  final Map<String, bool> _overrides = {};

  String _key(StarKind kind, String id) => '${kind.name}:$id';

  bool isStarred(StarKind kind, String id, bool fromServer) =>
      _overrides[_key(kind, id)] ?? fromServer;

  bool isTrackStarred(Track t) => isStarred(StarKind.track, t.id, t.starred);

  /// Schaltet den Favoriten-Status um und gibt den neuen Wert zurück.
  ///
  /// Die Anzeige wechselt SOFORT, die Meldung an den Server läuft danach
  /// ("optimistisch"). Schlägt sie fehl, wird die Anzeige zurückgedreht und
  /// der Fehler weitergereicht – so sieht der Nutzer nie einen Zustand, der
  /// am Server nicht stimmt.
  Future<bool> toggle(StarKind kind, String id, bool fromServer) async {
    final key = _key(kind, id);
    final before = _overrides[key] ?? fromServer;
    _overrides[key] = !before;
    notifyListeners();
    try {
      await rust.setStarred(kind: kind, id: id, starred: !before);
      return !before;
    } catch (_) {
      _overrides[key] = before;
      notifyListeners();
      rethrow;
    }
  }
}
