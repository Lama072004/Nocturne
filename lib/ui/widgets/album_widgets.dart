import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../actions.dart';
import '../screens/album_screen.dart';
import '../screens/artist_screen.dart';
import 'context_menu.dart';
import 'cover.dart';
import 'swipe_row.dart';

/// Gemeinsame Bausteine für Alben und Künstler: Karte (für waagrechte
/// Reihen und Raster), Listenzeile und kleine Kachel (Schnellzugriff auf
/// der Startseite). Alle öffnen dasselbe Menü und dieselbe Album-Seite.

void openAlbum(BuildContext context, String albumId) {
  Navigator.of(context).push(MaterialPageRoute(builder: (_) => AlbumScreen(albumId: albumId)));
}

void openArtist(BuildContext context, String artistId) {
  Navigator.of(context).push(MaterialPageRoute(builder: (_) => ArtistScreen(artistId: artistId)));
}

/// Lädt die Titel eines Albums: vom Server, und wenn das nicht geht
/// (offline), aus der lokalen Kopie der Bibliothek.
Future<List<Track>> albumTracks(String albumId) async {
  if (!AppState.instance.offline.value) {
    try {
      return (await rust.albumDetail(id: albumId)).tracks;
    } catch (_) {
      // Server nicht erreichbar -> unten die lokale Kopie versuchen.
    }
  }
  return local.localAlbumTracks(albumId: albumId);
}

/// Feste Höhe von Album-, Künstler- und Playlist-Zeilen (siehe
/// `kTrackRowHeight` in track_tile.dart für das Warum).
const double kCollectionRowHeight = 64;

/// Album als Karte: Cover mit Name und Künstler darunter.
class AlbumCard extends StatelessWidget {
  const AlbumCard({super.key, required this.album, this.size = 140});

  final Album album;

  /// Kantenlänge des Covers.
  final double size;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      borderRadius: BorderRadius.circular(6),
      onTap: () => openAlbum(context, album.id),
      onLongPress: () => showAlbumMenu(context, album, () => albumTracks(album.id)),
      child: SizedBox(
        width: size,
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Cover(coverId: album.coverArtId, size: size),
            const SizedBox(height: 6),
            Text(
              album.name,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodyMedium?.copyWith(fontWeight: FontWeight.w600),
            ),
            Text(
              album.artist ?? '',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
            ),
          ],
        ),
      ),
    );
  }
}

/// Album als Listenzeile mit Wischgesten und Drei-Punkte-Menü.
class AlbumRow extends StatelessWidget {
  const AlbumRow({super.key, required this.album});

  final Album album;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final app = AppState.instance;
    void openMenu() => showAlbumMenu(context, album, () => albumTracks(album.id));

    return ValueListenableBuilder(
      valueListenable: app.settings,
      builder: (context, settings, _) => SwipeRow(
        actions: settings.swipes.collectionRows,
        onAction: (a) => runSwipeAction(
          context,
          a,
          // Die Titel werden erst geladen, wenn wirklich gewischt wurde.
          // Für eine Liste aus 50 Alben vorab 50 Anfragen zu stellen, wäre
          // Verschwendung von Daten und Akku.
          tracks: () => albumTracks(album.id),
          favorite: (
            kind: StarKind.album,
            id: album.id,
            starred: app.favorites.isStarred(StarKind.album, album.id, album.starred),
          ),
        ),
        child: InkWell(
          onTap: () => openAlbum(context, album.id),
          onLongPress: openMenu,
          child: SizedBox(
            height: kCollectionRowHeight,
            child: Padding(
              padding: const EdgeInsets.only(left: 16),
              child: Row(
                children: [
                  Cover(coverId: album.coverArtId, size: 48),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          album.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodyLarge,
                        ),
                        const SizedBox(height: 2),
                        Text(
                          [
                            if (album.artist != null) album.artist!,
                            if (album.year != null) '${album.year}',
                          ].join(' · '),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                        ),
                      ],
                    ),
                  ),
                  MoreButton(onPressed: openMenu),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

/// Künstler als Listenzeile (rundes Symbol, wie bei Spotify üblich).
class ArtistRow extends StatelessWidget {
  const ArtistRow({super.key, required this.artist, this.subtitle});

  final Artist artist;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: () => openArtist(context, artist.id),
      child: SizedBox(
        height: kCollectionRowHeight,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Row(
            children: [
              // Rund = Künstler, eckig = Album: So erkennt man die Art schon
              // an der Form, ohne den Text zu lesen.
              ClipOval(child: Cover(coverId: artist.coverArtId, size: 48, radius: 0)),
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      artist.name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyLarge,
                    ),
                    if (subtitle != null) ...[
                      const SizedBox(height: 2),
                      Text(
                        subtitle!,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                      ),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Kleine Kachel für das Schnellzugriff-Raster der Startseite: kleines
/// Cover links, Name rechts (wie die Kacheln oben auf Spotifys Startseite).
class QuickTile extends StatelessWidget {
  const QuickTile({
    super.key,
    required this.coverId,
    required this.title,
    required this.onTap,
    this.onLongPress,
  });

  final String? coverId;
  final String title;
  final VoidCallback onTap;
  final VoidCallback? onLongPress;

  static const double height = 56;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Material(
      color: theme.colorScheme.surfaceContainer,
      borderRadius: BorderRadius.circular(6),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: onTap,
        onLongPress: onLongPress,
        child: SizedBox(
          height: height,
          child: Row(
            children: [
              Cover(coverId: coverId, size: height, radius: 0),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  title,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall?.copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              const SizedBox(width: 6),
            ],
          ),
        ),
      ),
    );
  }
}
