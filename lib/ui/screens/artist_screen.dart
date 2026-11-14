import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/detail_l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../actions.dart';
import '../widgets/album_widgets.dart';
import '../widgets/async_view.dart';
import '../widgets/track_tile.dart';
import 'track_list_screen.dart';

/// Alles, was die Seite braucht, egal woher es kommt.
class _ArtistData {
  const _ArtistData({
    required this.artist,
    required this.name,
    required this.albums,
    required this.appearsOn,
    required this.tracks,
  });

  /// Nur gesetzt, wenn vom Server geladen (für das Favoriten-Herz).
  final Artist? artist;

  /// Anzeigename; `null` = unbekannt (dann "Künstler").
  final String? name;

  /// Neueste Alben zuerst.
  final List<Album> albums;

  /// Alben ANDERER (z. B. Sampler), auf denen der Künstler nur mit einzelnen
  /// Titeln vertreten ist. Der Server nennt sie beim Künstler oft nicht;
  /// wir finden sie über die lokale Kopie der Bibliothek.
  final List<Album> appearsOn;

  /// Titel des Künstlers aus der lokalen Kopie, meistgehörte zuerst
  /// (für "Beliebt", Abspielen und Zufällig); kann leer sein.
  final List<Track> tracks;
}

/// Künstler-Seite: Name, Anzahl Alben, Favoriten-Herz, Abspielen/Zufällig
/// und die Alben als Liste.
class ArtistScreen extends StatefulWidget {
  const ArtistScreen({super.key, required this.artistId});

  final String artistId;

  @override
  State<ArtistScreen> createState() => _ArtistScreenState();
}

class _ArtistScreenState extends State<ArtistScreen> {
  late Future<_ArtistData> _future = _fetch();

  Future<_ArtistData> _fetch() async {
    Artist? artist;
    List<Album>? serverAlbums;
    Object? serverError;
    if (!AppState.instance.offline.value) {
      try {
        final d = await rust.artistDetail(id: widget.artistId);
        artist = d.artist;
        serverAlbums = d.albums;
      } catch (e) {
        serverError = e;
      }
    }

    // Lokale Kopie: Fallback für die Alben UND Quelle für "Abspielen"/"Zufällig".
    List<Album> localAlbums = const [];
    try {
      localAlbums = await local.localArtistAlbums(artistId: widget.artistId);
    } catch (_) {
      // Keine lokale Kopie -> Knöpfe bleiben ausgeblendet.
    }
    if (serverAlbums == null && localAlbums.isEmpty && serverError != null) {
      throw serverError;
    }

    // Eigene Alben: was der Server beim Künstler nennt. Ohne Server gilt ein
    // lokales Album als "eigenes", wenn der Künstler auch Album-Künstler ist.
    int byYear(Album a, Album b) => (b.year ?? 0).compareTo(a.year ?? 0);
    final List<Album> albums;
    if (serverAlbums != null) {
      albums = [...serverAlbums]..sort(byYear);
    } else {
      albums = [
        for (final a in localAlbums)
          if (a.artistId == widget.artistId) a,
      ]..sort(byYear);
    }
    // "Enthalten auf": lokale Alben, die nicht zu den eigenen gehören.
    // WARUM nötig: Bei Samplern ("Various Artists") liefert der Server für
    // den einzelnen Künstler eine leere Albenliste – die Seite war dann
    // komplett leer, obwohl Titel vorhanden sind.
    final ownIds = {for (final a in albums) a.id};
    final appearsOn = [
      for (final a in localAlbums)
        if (!ownIds.contains(a.id)) a,
    ]..sort(byYear);

    // Titel: aus eigenen Alben alle, aus fremden nur die des Künstlers.
    // (Früher landete hier der ganze Sampler in "Abspielen".)
    final tracks = <Track>[];
    for (final a in localAlbums) {
      try {
        final own = ownIds.contains(a.id);
        for (final t in await local.localAlbumTracks(albumId: a.id)) {
          if (own || t.artistId == widget.artistId) tracks.add(t);
        }
      } catch (_) {}
    }
    // Meistgehörte zuerst; bei Gleichstand bleibt die Album-Reihenfolge
    // (Dart sortiert Listen stabil genug für diesen Zweck nicht garantiert,
    // deshalb der Index als zweites Kriterium).
    final order = {for (var i = 0; i < tracks.length; i++) tracks[i].id: i};
    tracks.sort((a, b) {
      final c = (b.playCount ?? 0).compareTo(a.playCount ?? 0);
      return c != 0 ? c : order[a.id]!.compareTo(order[b.id]!);
    });

    String? name = artist?.name;
    if (name == null) {
      for (final t in tracks) {
        if (t.artistId == widget.artistId) {
          name = t.artist;
          break;
        }
      }
    }
    return _ArtistData(
      artist: artist,
      name: name ?? (albums.isEmpty ? null : albums.first.artist),
      albums: albums,
      appearsOn: appearsOn,
      tracks: tracks,
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(),
      body: AsyncView<_ArtistData>(
        future: _future,
        onRetry: () => setState(() {
          _future = _fetch();
        }),
        builder: (context, data) {
          final theme = Theme.of(context);
          final name = data.name ?? l.artistUnnamed;
          return CustomScrollView(
            slivers: [
              SliverToBoxAdapter(
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        name,
                        style: theme.textTheme.headlineMedium?.copyWith(
                          fontWeight: FontWeight.bold,
                        ),
                      ),
                      const SizedBox(height: 4),
                      Text(
                        [
                          if (data.albums.isNotEmpty || data.tracks.isEmpty)
                            l.albumsCount(data.albums.length),
                          if (data.tracks.isNotEmpty)
                            l.songs(data.tracks.length),
                        ].join(' · '),
                        style: theme.textTheme.bodyMedium?.copyWith(
                          color: theme.hintColor,
                        ),
                      ),
                      Row(
                        children: [
                          if (data.artist != null)
                            _FavoriteButton(artist: data.artist!),
                          const Spacer(),
                          // Ohne lokale Titel gäbe es nichts abzuspielen.
                          if (data.tracks.isNotEmpty)
                            DetailPlayButtons(tracks: data.tracks, name: name),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              // "Beliebt": die (bis zu fünf) meistgehörten Titel, wie bei
              // Spotify ganz oben. Tippen spielt alle Titel des Künstlers ab
              // diesem Titel.
              if (data.tracks.isNotEmpty) ...[
                SliverToBoxAdapter(child: _Section(l.t('Beliebt', 'Popular'))),
                SliverFixedExtentList.builder(
                  itemExtent: kTrackRowHeight,
                  itemCount: data.tracks.length.clamp(0, 5),
                  itemBuilder: (context, i) => TrackTile(
                    track: data.tracks[i],
                    onTap: () => AppState.instance.player.playContext(
                      data.tracks,
                      i,
                      name: name,
                    ),
                  ),
                ),
              ],
              if (data.albums.isNotEmpty) ...[
                SliverToBoxAdapter(child: _Section(l.t('Alben', 'Albums'))),
                // Feste Zeilenhöhe wie in allen langen Listen.
                SliverFixedExtentList.builder(
                  itemExtent: kCollectionRowHeight,
                  itemCount: data.albums.length,
                  itemBuilder: (context, i) => AlbumRow(album: data.albums[i]),
                ),
              ],
              if (data.appearsOn.isNotEmpty) ...[
                SliverToBoxAdapter(
                  child: _Section(l.t('Enthalten auf', 'Appears on')),
                ),
                SliverFixedExtentList.builder(
                  itemExtent: kCollectionRowHeight,
                  itemCount: data.appearsOn.length,
                  itemBuilder: (context, i) =>
                      AlbumRow(album: data.appearsOn[i]),
                ),
              ],
              const SliverToBoxAdapter(child: SizedBox(height: 96)),
            ],
          );
        },
      ),
    );
  }
}

/// Abschnittsüberschrift auf der Künstler-Seite.
class _Section extends StatelessWidget {
  const _Section(this.text);

  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
    child: Text(
      text,
      style: Theme.of(
        context,
      ).textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
    ),
  );
}

/// Herz für "Künstler favorisieren"; hört auf den Favoriten-Speicher.
class _FavoriteButton extends StatelessWidget {
  const _FavoriteButton({required this.artist});

  final Artist artist;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final favorites = AppState.instance.favorites;
    return ListenableBuilder(
      listenable: favorites,
      builder: (context, _) {
        final starred = favorites.isStarred(
          StarKind.artist,
          artist.id,
          artist.starred,
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
                StarKind.artist,
                artist.id,
                artist.starred,
              );
            } catch (e) {
              if (context.mounted) showToast(context, '$e');
            }
          },
        );
      },
    );
  }
}
