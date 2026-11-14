# Bauanleitung für Bildschirme

Für alle, die an `lib/ui/` arbeiten. Der Plan, WAS gebaut wird, steht in [UI_PLAN.md](UI_PLAN.md). Hier steht, WOMIT.

## Grundregeln

- **Kommentare auf Deutsch und reichlich.** Jede Datei erklärt oben ihren Zweck; wichtige Stellen erklären das WARUM (nicht, was die Zeile tut).
- **Texte nie fest im Widget.** Jeder sichtbare Text kommt aus `L.of(context)`. Neue Texte gehören in eine eigene Datei `lib/l10n/<bereich>_l10n.dart` als `extension <Bereich>L10n on L { String get xyz => t('Deutsch', 'English'); }`. `lib/l10n.dart` selbst nicht ändern.
- **Listen flüssig halten:** `ListView.builder`/`SliverList` mit fester Zeilenhöhe (`itemExtent: kTrackRowHeight` bzw. `kCollectionRowHeight`, beide 64). Kein `shrinkWrap` bei langen Listen, keine Liste in einer Liste.
- **Kein Hintergrund-Timer ohne Not** (Akku). Für die Wiedergabeposition gibt es `PositionBuilder`.
- **Berührungsflächen mindestens 48 × 48.** Jeder Icon-Knopf braucht `tooltip:` (wird auch vorgelesen).
- **Jeder leere Zustand und jeder Fehler nennt den nächsten Schritt** (`EmptyHint` mit `action`, `AsyncView` mit „Erneut versuchen“).
- **`setState` nie mit Pfeil-Funktion, die ein Future zuweist.** Falsch: `setState(() => _future = laden())`. Richtig: `setState(() { _future = laden(); })`. Sonst wirft Flutter im Debug-Modus einen Fehler und die Liste lädt nicht neu.
- **Nach einem `await` erst `mounted`/`context.mounted` prüfen**, bevor `context` benutzt wird.
- Nichts in `lib/src/rust/` ändern (erzeugter Code).

## Zustand (`lib/app_state.dart`)

```dart
final app = AppState.instance;
app.settings      // ValueNotifier<AppSettings>
app.loggedIn      // ValueNotifier<bool>
app.offline       // ValueNotifier<bool>  – true = nur Downloads abspielbar
app.network       // ValueNotifier<NetworkKind>
app.player        // PlayerController (ChangeNotifier)
app.favorites     // FavoriteStore (ChangeNotifier)
app.downloads     // DownloadController (ChangeNotifier)
```

### Player (`lib/player/player_controller.dart`)

```dart
app.player.snapshot            // QueueSnapshot?: current, userQueue, context, contextName, shuffle, repeat
app.player.currentTrack        // Track?
app.player.state               // NativePlayerState: isPlaying, isBuffering, durationMs, currentPosition
app.player.playContext(tracks, startIndex, name: 'Albumname');
app.player.playShuffled(tracks, name: ...);
app.player.addToQueue(tracks);
app.player.moveUpcoming(from, to);       // Index in userQueue ++ context
app.player.removeUpcoming(index);
app.player.insertUpcoming(index, track, intoUser: true);   // für "Rückgängig"
app.player.clearUserQueue();  app.player.clearContext();
app.player.skipToUpcoming(index);
app.player.togglePlay(); next(); previous(); seekTo(Duration);
app.player.toggleShuffle(); cycleRepeat();
app.player.setSleepTimer(minutes);       // >0 Minuten, -1 bis Titelende, 0 aus
app.player.sleepTimerActive
```

### Favoriten und Downloads

```dart
app.favorites.isTrackStarred(track);
app.favorites.isStarred(StarKind.album, album.id, album.starred);
await app.favorites.toggle(StarKind.track, track.id, track.starred);   // wirft bei Netzfehler

app.downloads.tracks            // List<Track>, neueste zuerst
app.downloads.status[songId]    // DownloadInfo? (state, percent, bytes)
app.downloads.isDownloaded(id); app.downloads.totalBytes;
await app.downloads.download(tracks, app.settings.value);   // -1 = Limit, 0 = schon da, n = gestartet
await app.downloads.remove(id); await app.downloads.removeAll();
```

### Merkzettel (`lib/prefs.dart`)

Für Ansicht, Sortierung, zuletzt gesucht usw. (bleibt über Neustarts erhalten):

```dart
Prefs.instance.get('library.view.albums') ?? 'list';
Prefs.instance.set('library.view.albums', 'grid');
Prefs.instance.getList('search.recent'); Prefs.instance.setList('search.recent', [...]);
// Prefs ist ein ChangeNotifier; noch nicht vorgeladene Schlüssel: await Prefs.instance.load(key)
```

## Daten aus Rust (`lib/src/rust/api/…`)

Alle Funktionen sind `Future`, außer wo „sofort“ steht.

```dart
import '../../src/rust/api/library.dart' as rust;   // Server
rust.recentAlbums(count:), rust.newestAlbums(count:), rust.albumDetail(id:) -> AlbumDetail(album, tracks)
rust.artistDetail(id:) -> ArtistDetail(artist, albums), rust.playlists(), rust.playlistDetail(id:) -> PlaylistDetail(playlist, tracks)
rust.starred() -> Starred(artists, albums, tracks), rust.search(query:) -> SearchResult(artists, albums, tracks)
rust.createPlaylist(name:, songIds:), rust.addToPlaylist(playlistId:, songIds:), rust.lyrics(songId:) -> Lyrics?
rust.coverUrl(coverId:, size:)   // sofort; normalerweise das Widget `Cover` benutzen

import '../../src/rust/api/local.dart' as local;    // lokale Kopie der Bibliothek: sofort da, geht offline
local.localAlbums(sort: local.AlbumSort.name | artist | yearNewest | addedNewest)
local.localArtists(), local.localAlbumTracks(albumId:), local.localArtistAlbums(artistId:)
local.localGenres() -> List<GenreCount(name, trackCount)>, local.localGenreTracks(genre:)
local.localSearch(query:) -> SearchResult, local.localRandomTracks(count:)

import '../../src/rust/api/smart.dart' as smart;
smart.syncLibrary() -> Anzahl Titel (füllt die lokale Kopie), smart.librarySyncInfo() -> (trackCount, lastSyncMs?)
smart.listSmartPlaylists(), smart.evaluateSmartPlaylist(id:) -> List<Track>

import '../../src/rust/api/stats.dart' as stats;
stats.listeningStats(days: 28) -> totalPlays, totalSecs (BigInt), topTracks/topArtists/topAlbums (StatEntry), days, hours
stats.recentPlays(limit:) -> List<PlayRecord(track, playedAtMs)>

import '../../src/rust/api/session.dart';
sessionInfo() -> SessionInfo(username, serverUrl, kind)?   // sofort
```

Typen stehen in `lib/src/rust/api/models.dart` (`Track`, `Album`, `Artist`, `Playlist`, …).

## Fertige Bausteine (`lib/ui/widgets/`)

| Baustein | Zweck |
|---|---|
| `Cover(coverId:, size:, radius:)` | Cover in passender Größe, mit Platzhalter und Cache |
| `TrackTile(track:, onTap:, showCover:, queueIndex:, trailing:, showDuration:)` | Titel-Zeile mit Wischgesten, Drei-Punkte-Menü, langem Drücken, Zustands-Symbolen und Offline-Abdunklung. Höhe `kTrackRowHeight`. |
| `AlbumRow(album:)`, `AlbumCard(album:, size:)`, `ArtistRow(artist:, subtitle:)`, `QuickTile(...)` | Zeilen und Karten für Alben/Künstler. `openAlbum(context, id)`, `openArtist(context, id)`, `albumTracks(id)` (Server, sonst lokal). |
| `showTrackMenu(context, track, queueIndex:)`, `showAlbumMenu(context, album, tracks)`, `MoreButton(onPressed:)` | Kontextmenü als Blatt von unten |
| `AsyncView<T>(future:, builder:, onRetry:)` | Ladekreis / Fehler mit „Erneut versuchen“ / Inhalt |
| `EmptyHint(icon:, text:, detail:, action:)` | Leerer Zustand mit nächstem Schritt |
| `PositionBuilder(builder:, interval:)` | Baut neu, solange Musik läuft und das Widget sichtbar ist |
| `ProfileButton()` | Runder Profil-Knopf; als `leading:` der `AppBar` mit `leadingWidth: 60` |
| `NocturneLogo(size:)` | Logo |
| `SwipeRow(actions:, onAction:, child:)` | Wischgesten für eigene Zeilenarten |

Aus `lib/ui/actions.dart`: `showToast(context, text)`, `runSwipeAction(...)`, `showAddToPlaylistSheet(context, tracks)`.
Aus `lib/theme/app_theme.dart`: `CoverColors.of(coverId)` liefert die Hauptfarbe eines Covers (`Future<Color?>`).
Aus `lib/ui/shell.dart`: `AppShell.goToTab(context, AppShell.tabLibrary)` springt zu einem Tab.
`TrackListScreen(title:, load:, pinPlaylistId:)` ist die allgemeine Seite „Liste von Titeln“.

## Prüfen

```bash
flutter analyze
```

Muss „No issues found!“ melden. Den Emulator bedient nur die Leitung.
