import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/detail_l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../../theme/app_theme.dart';
import '../actions.dart';
import '../widgets/album_widgets.dart';
import '../widgets/context_menu.dart';
import '../widgets/cover.dart';
import '../widgets/track_tile.dart';
import 'track_list_screen.dart';

/// Album-Angaben plus Titel, egal ob vom Server oder aus der lokalen Kopie.
class _AlbumData {
  const _AlbumData(this.album, this.tracks);
  final Album album;
  final List<Track> tracks;
}

/// Wird geworfen, wenn weder Server noch lokale Kopie das Album kennen.
class _AlbumUnavailable implements Exception {
  const _AlbumUnavailable();
}

/// Album-Seite im Spotify-Stil: großes Cover, Farbverlauf passend zum Cover,
/// Aktionszeile (Herz, Download, Menü, Shuffle, Play) und die Titelliste.
class AlbumScreen extends StatefulWidget {
  const AlbumScreen({super.key, required this.albumId});

  final String albumId;

  @override
  State<AlbumScreen> createState() => _AlbumScreenState();
}

class _AlbumScreenState extends State<AlbumScreen> {
  late Future<_AlbumData> _future;
  Color? _color;

  @override
  void initState() {
    super.initState();
    _load();
  }

  void _load() {
    _future = _fetch();
    // Farbe erst bestimmen, wenn das Album da ist (wir brauchen die Cover-ID).
    _future
        .then((d) async {
          final c = await CoverColors.of(d.album.coverArtId);
          if (mounted) setState(() => _color = c);
        })
        .catchError((_) {});
  }

  /// Zuerst der Server; offline oder bei Fehler die lokale Kopie, damit
  /// heruntergeladene Alben auch ohne Netz geöffnet werden können.
  Future<_AlbumData> _fetch() async {
    Object? serverError;
    if (!AppState.instance.offline.value) {
      try {
        final d = await rust.albumDetail(id: widget.albumId);
        return _AlbumData(d.album, d.tracks);
      } catch (e) {
        serverError = e;
      }
    }
    List<Track> tracks = const [];
    try {
      tracks = await local.localAlbumTracks(albumId: widget.albumId);
    } catch (_) {
      // Lokal nichts da -> unten Fehler melden.
    }
    if (tracks.isEmpty) throw serverError ?? const _AlbumUnavailable();
    // Album-Angaben aus den Titeln ableiten.
    final first = tracks.first;
    final secs = tracks.fold<int>(0, (s, t) => s + (t.durationSecs ?? 0));
    return _AlbumData(
      Album(
        id: widget.albumId,
        name: first.album ?? first.title,
        artist: first.artist,
        artistId: first.artistId,
        coverArtId: first.coverArtId,
        year: first.year,
        songCount: tracks.length,
        durationSecs: secs,
        starred: false,
      ),
      tracks,
    );
  }

  /// Verlauf-Startfarbe: Die Cover-Farbe ist dunkel. Im dunklen Design bleibt
  /// sie fast unverändert (helle Schrift lesbar), im hellen wird sie stark
  /// zur Hintergrundfarbe aufgehellt (dunkle Schrift lesbar).
  Color _top(ThemeData theme) {
    final bg = theme.scaffoldBackgroundColor;
    final c = _color;
    if (c == null) return bg;
    final dark = theme.brightness == Brightness.dark;
    return Color.lerp(bg, c, dark ? 0.9 : 0.35)!;
  }

  /// Sortiert nach CD und Titelnummer, falls es mehrere CDs gibt. Bei einer
  /// CD bleibt die Reihenfolge des Servers.
  List<Track> _ordered(List<Track> tracks) {
    final discs = {for (final t in tracks) t.discNumber ?? 1};
    if (discs.length < 2) return tracks;
    final sorted = [...tracks];
    sorted.sort((a, b) {
      final d = (a.discNumber ?? 1).compareTo(b.discNumber ?? 1);
      return d != 0 ? d : (a.trackNumber ?? 0).compareTo(b.trackNumber ?? 0);
    });
    return sorted;
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      // AppBar liegt ÜBER dem Farbverlauf, damit die Farbe bis ganz nach oben reicht.
      extendBodyBehindAppBar: true,
      appBar: AppBar(backgroundColor: Colors.transparent, elevation: 0),
      body: FutureBuilder<_AlbumData>(
        future: _future,
        builder: (context, snap) {
          if (snap.hasError) {
            final err = snap.error;
            return Center(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Padding(
                    padding: const EdgeInsets.all(24),
                    child: Text(
                      err is _AlbumUnavailable ? l.albumUnavailable : '$err',
                      textAlign: TextAlign.center,
                    ),
                  ),
                  FilledButton(
                    onPressed: () => setState(_load),
                    child: Text(l.retry),
                  ),
                ],
              ),
            );
          }
          final data = snap.data;
          if (data == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return _content(context, l, data.album, _ordered(data.tracks));
        },
      ),
    );
  }

  Widget _content(BuildContext context, L l, Album album, List<Track> tracks) {
    final theme = Theme.of(context);
    final player = AppState.instance.player;
    final bg = theme.scaffoldBackgroundColor;
    final secs = tracks.fold<int>(0, (s, t) => s + (t.durationSecs ?? 0));
    final favorites = AppState.instance.favorites;

    // Gruppen pro CD (bei nur einer CD: eine Gruppe ohne Überschrift).
    final groups = <int, List<int>>{}; // CD -> Indizes in `tracks`
    for (var i = 0; i < tracks.length; i++) {
      groups.putIfAbsent(tracks[i].discNumber ?? 1, () => []).add(i);
    }
    final multiDisc = groups.length > 1;

    return CustomScrollView(
      slivers: [
        SliverToBoxAdapter(
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 400),
            decoration: BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.topCenter,
                end: Alignment.bottomCenter,
                colors: [_top(theme), bg],
              ),
            ),
            padding: EdgeInsets.fromLTRB(
              16,
              MediaQuery.paddingOf(context).top + kToolbarHeight,
              16,
              0,
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Center(
                  child: Cover(coverId: album.coverArtId, size: 240, radius: 6),
                ),
                const SizedBox(height: 20),
                Text(
                  album.name,
                  style: theme.textTheme.headlineSmall?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
                if (album.artist != null) ...[
                  const SizedBox(height: 4),
                  // Künstler antippbar, wenn wir seine ID kennen.
                  InkWell(
                    onTap: album.artistId == null
                        ? null
                        : () => openArtist(context, album.artistId!),
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 4),
                      child: Text(
                        album.artist!,
                        style: theme.textTheme.titleMedium?.copyWith(
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                    ),
                  ),
                ],
                Text(
                  [
                    if (album.year != null) '${album.year}',
                    l.songs(tracks.length),
                    if (secs > 0) l.totalLength(secs),
                  ].join(' · '),
                  style: theme.textTheme.bodyMedium?.copyWith(
                    color: theme.hintColor,
                  ),
                ),
                const SizedBox(height: 8),
                // Aktionszeile: links Herz, Download, Menü; rechts Shuffle + Play.
                Row(
                  children: [
                    ListenableBuilder(
                      listenable: favorites,
                      builder: (context, _) {
                        final starred = favorites.isStarred(
                          StarKind.album,
                          album.id,
                          album.starred,
                        );
                        return IconButton(
                          tooltip: starred ? l.removeFavorite : l.addFavorite,
                          icon: Icon(
                            starred ? Icons.favorite : Icons.favorite_border,
                            color: starred ? theme.colorScheme.primary : null,
                          ),
                          onPressed: () async {
                            try {
                              await favorites.toggle(
                                StarKind.album,
                                album.id,
                                album.starred,
                              );
                            } catch (e) {
                              if (context.mounted) showToast(context, '$e');
                            }
                          },
                        );
                      },
                    ),
                    DetailDownloadButton(tracks: tracks),
                    MoreButton(
                      // Titel sind schon geladen, das Menü braucht keinen
                      // zweiten Abruf.
                      onPressed: () =>
                          showAlbumMenu(context, album, () async => tracks),
                    ),
                    const Spacer(),
                    DetailPlayButtons(tracks: tracks, name: album.name),
                  ],
                ),
              ],
            ),
          ),
        ),
        for (final entry in groups.entries) ...[
          if (multiDisc)
            SliverToBoxAdapter(
              child: Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                child: Row(
                  children: [
                    Icon(Icons.album_outlined, size: 18, color: theme.hintColor),
                    const SizedBox(width: 8),
                    Text(
                      l.discLabel(entry.key),
                      style: theme.textTheme.titleSmall?.copyWith(
                        color: theme.hintColor,
                      ),
                    ),
                  ],
                ),
              ),
            ),
          // Pro CD eine eigene Liste mit fester Zeilenhöhe: Überschriften
          // dazwischen brechen die Höhenberechnung so nicht.
          SliverFixedExtentList.builder(
            itemExtent: kTrackRowHeight,
            itemCount: entry.value.length,
            itemBuilder: (context, n) {
              final i = entry.value[n];
              return TrackTile(
                track: tracks[i],
                showCover: false,
                // Tippen startet das Album ab diesem Titel. Die eigene
                // Warteschlange bleibt dabei erhalten (Spotify-Verhalten).
                onTap: () => player.playContext(tracks, i, name: album.name),
              );
            },
          ),
        ],
        // Platz, damit der letzte Titel nicht unter dem Mini-Player liegt.
        const SliverToBoxAdapter(child: SizedBox(height: 96)),
      ],
    );
  }
}
