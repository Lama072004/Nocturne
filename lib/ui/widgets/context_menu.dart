import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../src/rust/api/models.dart';
import '../../src/rust/api/settings.dart';
import '../actions.dart';
import 'cover.dart';
import 'swipe_row.dart';

/// Kontextmenü als Blatt von unten, wie bei Spotify (Drei-Punkte-Menü oder
/// langes Drücken auf eine Zeile).
///
/// WARUM zusätzlich zu den Wischgesten?
/// Wischgesten sind schnell, aber unsichtbar: Wer sie nicht kennt, findet
/// die Aktionen nie. Das Menü zeigt ALLE Aktionen mit Namen. So ist die App
/// auch ohne Erklärung bedienbar, und die Gesten bleiben die Abkürzung für
/// Geübte.
///
/// Die Aktionen selbst laufen über [runSwipeAction] – dieselbe Stelle wie
/// bei den Gesten. Eine Aktion verhält sich also immer gleich, egal wie sie
/// ausgelöst wurde.

/// Kopfzeile des Menüs: Cover, Titel, Untertitel.
class _MenuHeader extends StatelessWidget {
  const _MenuHeader({required this.coverId, required this.title, this.subtitle});

  final String? coverId;
  final String title;
  final String? subtitle;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
      child: Row(
        children: [
          Cover(coverId: coverId, size: 52),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.titleMedium?.copyWith(fontWeight: FontWeight.bold),
                ),
                if (subtitle != null)
                  Text(
                    subtitle!,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// Öffnet das Menü für einen einzelnen Titel.
///
/// [queueIndex] ist gesetzt, wenn das Menü aus der Warteschlange kommt; dann
/// gibt es zusätzlich "Aus Warteschlange entfernen".
Future<void> showTrackMenu(BuildContext context, Track track, {int? queueIndex}) {
  final l = L.of(context);
  final app = AppState.instance;
  final starred = app.favorites.isTrackStarred(track);
  final downloaded = app.downloads.isKnown(track.id);

  // Reihenfolge wie bei Spotify: das Häufigste zuerst.
  final entries = <(SwipeAction, IconData, String)>[
    (SwipeAction.addToQueue, Icons.queue_music, l.swipeActionName(SwipeAction.addToQueue)),
    (
      SwipeAction.toggleFavorite,
      starred ? Icons.favorite : Icons.favorite_border,
      starred ? l.removeFavorite : l.addFavorite,
    ),
    (SwipeAction.addToPlaylist, Icons.playlist_add, l.addToPlaylistTitle),
    if (!downloaded) (SwipeAction.download, Icons.download_outlined, l.swipeActionName(SwipeAction.download)),
    if (track.albumId != null) (SwipeAction.goToAlbum, Icons.album_outlined, l.swipeActionName(SwipeAction.goToAlbum)),
    if (track.artistId != null)
      (SwipeAction.goToArtist, Icons.person_outline, l.swipeActionName(SwipeAction.goToArtist)),
    if (queueIndex != null)
      (SwipeAction.removeFromQueue, Icons.remove_circle_outline, l.removeFromQueueAction),
  ];

  return _showMenu(
    context,
    header: _MenuHeader(coverId: track.coverArtId, title: track.title, subtitle: track.artist),
    entries: entries,
    run: (action) => runSwipeAction(
      context,
      action,
      tracks: () async => [track],
      queueIndex: queueIndex,
      favorite: (kind: StarKind.track, id: track.id, starred: starred),
    ),
  );
}

/// Öffnet das Menü für ein Album (alle Titel auf einmal).
Future<void> showAlbumMenu(
  BuildContext context,
  Album album,
  Future<List<Track>> Function() tracks,
) {
  final l = L.of(context);
  final app = AppState.instance;
  final starred = app.favorites.isStarred(StarKind.album, album.id, album.starred);
  return _showMenu(
    context,
    header: _MenuHeader(coverId: album.coverArtId, title: album.name, subtitle: album.artist),
    entries: [
      (SwipeAction.addToQueue, Icons.queue_music, l.swipeActionName(SwipeAction.addToQueue)),
      (
        SwipeAction.toggleFavorite,
        starred ? Icons.favorite : Icons.favorite_border,
        starred ? l.removeFavorite : l.addFavorite,
      ),
      (SwipeAction.download, Icons.download_outlined, l.swipeActionName(SwipeAction.download)),
      (SwipeAction.addToPlaylist, Icons.playlist_add, l.addToPlaylistTitle),
      if (album.artistId != null)
        (SwipeAction.goToArtist, Icons.person_outline, l.swipeActionName(SwipeAction.goToArtist)),
    ],
    run: (action) => runSwipeAction(
      context,
      action,
      tracks: tracks,
      favorite: (kind: StarKind.album, id: album.id, starred: starred),
    ),
  );
}

Future<void> _showMenu(
  BuildContext context, {
  required Widget header,
  required List<(SwipeAction, IconData, String)> entries,
  required void Function(SwipeAction action) run,
}) {
  return showModalBottomSheet<void>(
    context: context,
    // Über Tab-Leiste und Mini-Player legen.
    useRootNavigator: true,
    showDragHandle: true,
    isScrollControlled: true,
    builder: (sheetContext) => SafeArea(
      child: ListView(
        shrinkWrap: true,
        children: [
          header,
          const Divider(height: 1),
          for (final (action, icon, label) in entries)
            ListTile(
              leading: Icon(icon),
              title: Text(label),
              onTap: () {
                // Erst das Blatt schließen, dann die Aktion ausführen: Die
                // Aktion braucht den Context der Seite darunter (für
                // Navigation und Hinweise), nicht den des Blatts.
                Navigator.of(sheetContext).pop();
                run(action);
              },
            ),
          const SizedBox(height: 8),
        ],
      ),
    ),
  );
}

/// Drei-Punkte-Knopf für Zeilen. Eigene Klasse, damit er überall gleich
/// aussieht und gleich groß ist (48 px Touch-Ziel).
class MoreButton extends StatelessWidget {
  const MoreButton({super.key, required this.onPressed});

  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) => IconButton(
        tooltip: L.of(context).moreOptions,
        icon: const Icon(Icons.more_vert),
        onPressed: onPressed,
      );
}

/// Nur damit die Icon-Zuordnung der Wischgesten an EINER Stelle bleibt und
/// hier wiederverwendet werden kann.
IconData iconForAction(SwipeAction action) => swipeActionIcon(action);
