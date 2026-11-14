import '../app_state.dart';
import '../src/rust/api/library.dart' as rust;
import '../src/rust/api/models.dart';
import '../src/rust/api/smart.dart' as smart;

/// Hält ausgewählte Inhalte automatisch heruntergeladen:
///  - Favoriten
///  - vorgemerkte Server-Playlists
///  - Smart-Playlists mit "automatisch herunterladen"
///
/// Jede der drei Arten lässt sich in den Einstellungen einzeln schalten.
///
/// AKKU: Es gibt hier KEINEN Timer. Der Abgleich läuft nur bei bestimmten
/// Anlässen: App-Start, Wechsel ins WLAN und nach dem Ändern der
/// Einstellungen. Die eigentlichen Downloads übernimmt danach der
/// Android-Dienst, auch wenn die App geschlossen wird.
class AutoSync {
  AutoSync._();

  /// Verhindert, dass zwei Abgleiche gleichzeitig laufen (z. B. App-Start
  /// und Netzwechsel kurz hintereinander).
  static bool _running = false;

  static Future<void> run() async {
    final app = AppState.instance;
    final s = app.settings.value;
    final anyEnabled =
        s.autoSyncFavorites || s.autoSyncPlaylists || s.autoSyncSmart;
    if (_running || !anyEnabled || !app.loggedIn.value) return;
    // Ohne Netz geht nichts. In mobilen Daten nur, wenn der Nutzer Downloads
    // dort erlaubt hat – sonst würden schon die Listen-Abfragen
    // Datenvolumen kosten, ohne dass danach etwas geladen werden darf.
    final net = app.network.value;
    if (net == rust.NetworkKind.none) return;
    if (net == rust.NetworkKind.mobile && s.downloadWifiOnly) return;

    _running = true;
    try {
      final wanted = <Track>[];

      if (s.autoSyncFavorites) {
        await _collect(wanted, () async => (await rust.starred()).tracks);
      }
      if (s.autoSyncPlaylists) {
        for (final p in await smart.pinnedPlaylists()) {
          await _collect(
            wanted,
            () async => (await rust.playlistDetail(id: p.id)).tracks,
          );
        }
      }
      if (s.autoSyncSmart) {
        for (final p in await smart.listSmartPlaylists()) {
          if (!p.autoDownload) continue;
          await _collect(wanted, () => smart.evaluateSmartPlaylist(id: p.id));
        }
      }

      // `download` überspringt bereits vorhandene Titel selbst.
      if (wanted.isNotEmpty) await app.downloads.download(wanted, s);
    } catch (_) {
      // Ein fehlgeschlagener Abgleich ist nicht schlimm: Beim nächsten
      // Anlass wird er wiederholt.
    } finally {
      _running = false;
    }
  }

  /// Lädt eine Liste und hängt sie an. Schlägt EINE Quelle fehl (z. B. eine
  /// am Server gelöschte Playlist), laufen die anderen trotzdem weiter.
  static Future<void> _collect(
    List<Track> into,
    Future<List<Track>> Function() load,
  ) async {
    try {
      into.addAll(await load());
    } catch (_) {}
  }
}
