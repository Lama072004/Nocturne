import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../../src/rust/api/smart.dart' as smart;
import '../../src/rust/api/stats.dart' as stats;
import '../actions.dart';
import '../widgets/album_widgets.dart';
import '../widgets/async_view.dart';
import '../widgets/context_menu.dart';
import '../widgets/profile_button.dart';
import '../widgets/track_tile.dart';
import 'downloads_screen.dart';
import 'track_list_screen.dart';

/// Startseite.
///
/// Feste Reihen statt eines wechselnden "Feeds": Was gestern oben stand,
/// steht heute wieder dort. (Ein Home, das sich ständig umbaut, ist einer
/// der häufigsten Kritikpunkte an Spotify.)
///
/// Aufbau von oben nach unten:
///  1. Begrüßung + Profil-Knopf
///  2. Schnell-Aktionen (Alles zufällig, Lieblingssongs, Downloads)
///  3. Schnellzugriff-Raster: zuletzt gehörte Alben
///  4. Neu hinzugefügt
///  5. Deine Top-Titel (aus der eigenen Statistik)
///  6. Smart-Playlists
///
/// Jede Reihe lädt für sich. Schlägt eine fehl, zeigt nur sie den Fehler.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  /// Wird bei "Ziehen zum Aktualisieren" hochgezählt. Die Reihen hängen als
  /// Schlüssel daran und laden dadurch alle neu.
  int _generation = 0;

  String _greeting(L l) {
    final hour = DateTime.now().hour;
    if (hour < 11) return l.goodMorning;
    if (hour < 18) return l.goodAfternoon;
    return l.goodEvening;
  }

  /// "Alles zufällig": zufällige Titel aus der GANZEN Bibliothek.
  Future<void> _shuffleAll() async {
    final l = L.of(context);
    try {
      var tracks = await local.localRandomTracks(count: 200);
      if (tracks.isEmpty) {
        // Die lokale Titel-Liste fehlt noch (erster Start): einmal laden.
        if (mounted) showToast(context, l.t('Titel-Liste wird geladen …', 'Loading track list …'));
        await smart.syncLibrary();
        tracks = await local.localRandomTracks(count: 200);
      }
      // Die Titel sind schon zufällig gewählt und gereiht; Shuffle muss
      // dafür nicht eingeschaltet werden.
      await AppState.instance.player.playContext(tracks, 0, name: l.shuffleAll);
    } catch (e) {
      if (mounted) showToast(context, '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);

    return Scaffold(
      appBar: AppBar(
        leading: const ProfileButton(),
        leadingWidth: 60,
        title: Text(_greeting(l), style: const TextStyle(fontWeight: FontWeight.bold)),
      ),
      body: ValueListenableBuilder<bool>(
        valueListenable: AppState.instance.offline,
        builder: (context, offline, _) {
          if (offline) {
            // Offline kann die Startseite nichts vom Server laden. Statt
            // Fehlermeldungen zeigen wir, wo die Musik jetzt zu finden ist.
            return EmptyHint(
              icon: Icons.cloud_off_outlined,
              text: l.offlineNotAvailable,
              detail: l.offlineBanner,
              action: FilledButton.icon(
                icon: const Icon(Icons.download_done),
                label: Text(l.downloads),
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => const DownloadsScreen()),
                ),
              ),
            );
          }
          return RefreshIndicator(
            // Nach unten ziehen lädt neu. Automatisches Nachladen im
            // Hintergrund gibt es bewusst nicht (Akku).
            onRefresh: () async => setState(() => _generation++),
            child: ListView(
              children: [
                // ---------------- Schnell-Aktionen ----------------
                SizedBox(
                  height: 48,
                  child: ListView(
                    scrollDirection: Axis.horizontal,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    children: [
                      _ActionChip(icon: Icons.shuffle, label: l.shuffleAll, onTap: _shuffleAll),
                      _ActionChip(
                        icon: Icons.favorite,
                        label: l.likedSongs,
                        onTap: () => Navigator.of(context).push(
                          MaterialPageRoute(
                            builder: (_) => TrackListScreen(
                              title: l.likedSongs,
                              load: () async => (await rust.starred()).tracks,
                            ),
                          ),
                        ),
                      ),
                      _ActionChip(
                        icon: Icons.download_done,
                        label: l.downloads,
                        onTap: () => Navigator.of(context).push(
                          MaterialPageRoute(builder: (_) => const DownloadsScreen()),
                        ),
                      ),
                    ],
                  ),
                ),
                _RecentGrid(key: ValueKey('recent$_generation')),
                _AlbumShelf(
                  key: ValueKey('new$_generation'),
                  title: l.newlyAdded,
                  load: () => rust.newestAlbums(count: 20),
                ),
                _TopTracks(key: ValueKey('top$_generation')),
                _SmartShelf(key: ValueKey('smart$_generation')),
                SizedBox(height: 24, child: ColoredBox(color: theme.scaffoldBackgroundColor)),
              ],
            ),
          );
        },
      ),
    );
  }
}

class _ActionChip extends StatelessWidget {
  const _ActionChip({required this.icon, required this.label, required this.onTap});

  final IconData icon;
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 4),
    child: ActionChip(avatar: Icon(icon, size: 18), label: Text(label), onPressed: onTap),
  );
}

/// Überschrift einer Reihe.
class _SectionTitle extends StatelessWidget {
  const _SectionTitle(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 24, 16, 12),
    child: Text(
      text,
      style: Theme.of(context).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.bold),
    ),
  );
}

/// Kleine Fehlermeldung innerhalb einer Reihe, mit "Erneut versuchen".
class _RowError extends StatelessWidget {
  const _RowError({required this.error, required this.onRetry});

  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Row(
        children: [
          Expanded(
            child: Text(
              '$error',
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall?.copyWith(color: theme.colorScheme.error),
            ),
          ),
          TextButton(onPressed: onRetry, child: Text(L.of(context).retry)),
        ],
      ),
    );
  }
}

/// Schnellzugriff: die zuletzt gehörten Alben als Raster aus kleinen
/// Kacheln (zwei Spalten, höchstens vier Zeilen).
class _RecentGrid extends StatefulWidget {
  const _RecentGrid({super.key});

  @override
  State<_RecentGrid> createState() => _RecentGridState();
}

class _RecentGridState extends State<_RecentGrid> {
  late Future<List<Album>> _future = rust.recentAlbums(count: 8);

  @override
  Widget build(BuildContext context) {
    return FutureBuilder<List<Album>>(
      future: _future,
      builder: (context, snap) {
        if (snap.hasError) {
          return Padding(
            padding: const EdgeInsets.only(top: 12),
            child: _RowError(
              error: snap.error!,
              onRetry: () => setState(() {
                _future = rust.recentAlbums(count: 8);
              }),
            ),
          );
        }
        final albums = snap.data;
        // Platz freihalten, solange geladen wird: Die Seite springt dann
        // nicht, wenn die Kacheln erscheinen.
        final count = albums?.length ?? 8;
        if (count == 0) return const SizedBox.shrink();
        final rows = (count + 1) ~/ 2;
        return Padding(
          padding: const EdgeInsets.fromLTRB(16, 12, 16, 0),
          child: Column(
            children: [
              for (var r = 0; r < rows; r++)
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Row(
                    children: [
                      for (var c = 0; c < 2; c++) ...[
                        if (c == 1) const SizedBox(width: 8),
                        Expanded(child: _tile(context, albums, r * 2 + c)),
                      ],
                    ],
                  ),
                ),
            ],
          ),
        );
      },
    );
  }

  Widget _tile(BuildContext context, List<Album>? albums, int i) {
    if (albums == null) {
      // Platzhalter in Kachelgröße während des Ladens.
      return Container(
        height: QuickTile.height,
        decoration: BoxDecoration(
          color: Theme.of(context).colorScheme.surfaceContainer,
          borderRadius: BorderRadius.circular(6),
        ),
      );
    }
    if (i >= albums.length) return const SizedBox(height: QuickTile.height);
    final album = albums[i];
    return QuickTile(
      coverId: album.coverArtId,
      title: album.name,
      onTap: () => openAlbum(context, album.id),
      onLongPress: () => showAlbumMenu(context, album, () => albumTracks(album.id)),
    );
  }
}

/// Eine Überschrift mit waagrecht scrollender Album-Reihe.
class _AlbumShelf extends StatefulWidget {
  const _AlbumShelf({super.key, required this.title, required this.load});

  final String title;
  final Future<List<Album>> Function() load;

  @override
  State<_AlbumShelf> createState() => _AlbumShelfState();
}

class _AlbumShelfState extends State<_AlbumShelf> {
  late Future<List<Album>> _future = widget.load();

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _SectionTitle(widget.title),
        SizedBox(
          height: 190,
          child: FutureBuilder<List<Album>>(
            future: _future,
            builder: (context, snap) {
              if (snap.hasError) {
                return _RowError(
                  error: snap.error!,
                  onRetry: () => setState(() {
                    _future = widget.load();
                  }),
                );
              }
              final albums = snap.data;
              if (albums == null) return const Center(child: CircularProgressIndicator());
              return ListView.separated(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 16),
                itemCount: albums.length,
                separatorBuilder: (_, _) => const SizedBox(width: 12),
                itemBuilder: (_, i) => AlbumCard(album: albums[i]),
              );
            },
          ),
        ),
      ],
    );
  }
}

/// "Deine Top-Titel": die meistgehörten Titel der letzten Zeit.
///
/// Gezählt wird aus dem eigenen Verlauf der App. Solange es noch keinen
/// Verlauf gibt, erscheint die Reihe gar nicht (keine leere Überschrift).
class _TopTracks extends StatefulWidget {
  const _TopTracks({super.key});

  @override
  State<_TopTracks> createState() => _TopTracksState();
}

class _TopTracksState extends State<_TopTracks> {
  late final Future<List<Track>> _future = _load();

  Future<List<Track>> _load() async {
    final plays = await stats.recentPlays(limit: 400);
    // Wiedergaben je Titel zählen; die Reihenfolge des ersten Auftretens
    // (= zuletzt gehört) entscheidet bei Gleichstand.
    final counts = <String, int>{};
    final byId = <String, Track>{};
    for (final p in plays) {
      counts[p.track.id] = (counts[p.track.id] ?? 0) + 1;
      byId.putIfAbsent(p.track.id, () => p.track);
    }
    final ids = byId.keys.toList()..sort((a, b) => counts[b]!.compareTo(counts[a]!));
    return [for (final id in ids.take(5)) byId[id]!];
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return FutureBuilder<List<Track>>(
      future: _future,
      builder: (context, snap) {
        final tracks = snap.data;
        if (tracks == null || tracks.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _SectionTitle(l.yourTopTracks),
            for (var i = 0; i < tracks.length; i++)
              TrackTile(
                track: tracks[i],
                onTap: () =>
                    AppState.instance.player.playContext(tracks, i, name: l.yourTopTracks),
              ),
          ],
        );
      },
    );
  }
}

/// Die eigenen Smart-Playlists als waagrechte Reihe von Karten.
class _SmartShelf extends StatefulWidget {
  const _SmartShelf({super.key});

  @override
  State<_SmartShelf> createState() => _SmartShelfState();
}

class _SmartShelfState extends State<_SmartShelf> {
  late final Future<List<smart.SmartPlaylist>> _future = smart.listSmartPlaylists();

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    return FutureBuilder<List<smart.SmartPlaylist>>(
      future: _future,
      builder: (context, snap) {
        final playlists = snap.data;
        if (playlists == null || playlists.isEmpty) return const SizedBox.shrink();
        return Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            _SectionTitle(l.smartPlaylists),
            SizedBox(
              height: 96,
              child: ListView.separated(
                scrollDirection: Axis.horizontal,
                padding: const EdgeInsets.symmetric(horizontal: 16),
                itemCount: playlists.length,
                separatorBuilder: (_, _) => const SizedBox(width: 12),
                itemBuilder: (context, i) {
                  final p = playlists[i];
                  return Material(
                    color: theme.colorScheme.surfaceContainer,
                    borderRadius: BorderRadius.circular(10),
                    clipBehavior: Clip.antiAlias,
                    child: InkWell(
                      onTap: () => Navigator.of(context).push(
                        MaterialPageRoute(
                          builder: (_) => TrackListScreen(
                            title: p.name,
                            load: () => smart.evaluateSmartPlaylist(id: p.id),
                          ),
                        ),
                      ),
                      child: SizedBox(
                        width: 150,
                        child: Padding(
                          padding: const EdgeInsets.all(12),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Icon(Icons.auto_awesome, color: theme.colorScheme.primary),
                              const Spacer(),
                              Text(
                                p.name,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                                style: theme.textTheme.bodyMedium?.copyWith(
                                  fontWeight: FontWeight.w600,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ),
                  );
                },
              ),
            ),
          ],
        );
      },
    );
  }
}
