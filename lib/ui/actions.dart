import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_state.dart';
import '../l10n.dart';
import '../src/rust/api/library.dart' as rust;
import '../src/rust/api/models.dart';
import '../src/rust/api/settings.dart';
import 'screens/album_screen.dart';
import 'screens/artist_screen.dart';

/// Kurze Meldung am unteren Rand (wie die "Toast"-Hinweise bei Spotify).
void showToast(BuildContext context, String text) {
  ScaffoldMessenger.of(context)
    ..hideCurrentSnackBar()
    ..showSnackBar(
      SnackBar(
        content: Text(text),
        duration: const Duration(milliseconds: 1400),
        behavior: SnackBarBehavior.floating,
      ),
    );
}

/// Führt eine Wisch-Aktion aus.
///
/// Eine zentrale Stelle für ALLE Zeilenarten: Welche Geste welche Aktion
/// auslöst, steht in den Einstellungen. Was die Aktion dann tut, steht nur
/// hier. So verhält sich z. B. "Zur Warteschlange" überall gleich.
///
/// [tracks] sind die betroffenen Titel (bei einer Titel-Zeile einer, bei
/// einer Album-Zeile alle). Sie werden erst bei Bedarf geladen.
/// [queueIndex] ist nur in der Warteschlange gesetzt.
/// [favorite] beschreibt, WAS favorisiert wird (Titel, Album oder Künstler).
Future<void> runSwipeAction(
  BuildContext context,
  SwipeAction action, {
  required Future<List<Track>> Function() tracks,
  int? queueIndex,
  ({StarKind kind, String id, bool starred})? favorite,
}) async {
  final l = L.of(context);
  final app = AppState.instance;
  // Alles, was `context` braucht, VOR dem ersten `await` holen: Danach
  // könnte die Zeile schon nicht mehr im Baum sein (weggescrollt).
  final messenger = ScaffoldMessenger.of(context);
  final navigator = Navigator.of(context);

  /// Hinweis am unteren Rand. Mit [onUndo] bekommt er einen Knopf
  /// "Rückgängig" und bleibt etwas länger stehen, damit man ihn auch trifft.
  void toast(String text, {VoidCallback? onUndo}) {
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(text),
          duration: Duration(milliseconds: onUndo == null ? 1400 : 4000),
          behavior: SnackBarBehavior.floating,
          action: onUndo == null ? null : SnackBarAction(label: l.undo, onPressed: onUndo),
        ),
      );
  }

  try {
    switch (action) {
      case SwipeAction.none:
        return;

      case SwipeAction.addToQueue:
        final added = await tracks();
        if (added.isEmpty) return;
        final wasPlaying = app.player.hasTrack;
        await app.player.addToQueue(added);
        // Leichtes Vibrieren als Bestätigung: Die Aktion wird meist per
        // Wischgeste ausgelöst, ohne dass man auf den Hinweis schaut.
        HapticFeedback.lightImpact();
        toast(
          l.addedToQueue,
          // Rückgängig nimmt genau die eben angehängten Titel wieder weg.
          // Sie stehen am Ende der eigenen Warteschlange. (Lief vorher
          // nichts, wurden die Titel sofort abgespielt – dann gibt es
          // nichts zurückzunehmen.)
          onUndo: !wasPlaying
              ? null
              : () async {
                  for (var i = 0; i < added.length; i++) {
                    final userLen = app.player.snapshot?.userQueue.length ?? 0;
                    if (userLen == 0) break;
                    await app.player.removeUpcoming(userLen - 1);
                  }
                },
        );

      case SwipeAction.removeFromQueue:
        if (queueIndex != null) {
          // Vor dem Entfernen merken, was wo stand – für "Rückgängig".
          final snap = app.player.snapshot;
          final userLen = snap?.userQueue.length ?? 0;
          final upcoming = [...?snap?.userQueue, ...?snap?.context];
          final removed = queueIndex < upcoming.length ? upcoming[queueIndex].track : null;
          await app.player.removeUpcoming(queueIndex);
          HapticFeedback.lightImpact();
          toast(
            l.removedFromQueue,
            onUndo: removed == null
                ? null
                : () => app.player.insertUpcoming(
                    queueIndex,
                    removed,
                    intoUser: queueIndex < userLen,
                  ),
          );
        }

      case SwipeAction.toggleFavorite:
        if (favorite == null) return;
        final now = await app.favorites.toggle(
          favorite.kind,
          favorite.id,
          favorite.starred,
        );
        toast(now ? l.addedToFavorites : l.removedFromFavorites);

      case SwipeAction.download:
        final n = await app.downloads.download(
          await tracks(),
          app.settings.value,
        );
        toast(switch (n) {
          -1 => l.downloadLimitReached,
          0 => l.alreadyDownloaded,
          _ => l.downloadStarted,
        });

      case SwipeAction.addToPlaylist:
        final list = await tracks();
        if (list.isEmpty || !context.mounted) return;
        await showAddToPlaylistSheet(context, list);

      case SwipeAction.goToAlbum:
        final list = await tracks();
        final albumId = list.isEmpty ? null : list.first.albumId;
        if (albumId != null) {
          navigator.push(
            MaterialPageRoute(builder: (_) => AlbumScreen(albumId: albumId)),
          );
        }

      case SwipeAction.goToArtist:
        final list = await tracks();
        final artistId = list.isEmpty ? null : list.first.artistId;
        if (artistId != null) {
          navigator.push(
            MaterialPageRoute(builder: (_) => ArtistScreen(artistId: artistId)),
          );
        }
    }
  } catch (e) {
    // Fehler (meist: kein Netz) sichtbar machen statt still zu schlucken.
    toast('$e');
  }
}

/// Blatt von unten: bestehende Playlist wählen oder neue anlegen.
Future<void> showAddToPlaylistSheet(
  BuildContext context,
  List<Track> tracks,
) async {
  final l = L.of(context);
  final messenger = ScaffoldMessenger.of(context);
  final ids = [for (final t in tracks) t.id];

  void done(String name) {
    messenger
      ..hideCurrentSnackBar()
      ..showSnackBar(
        SnackBar(
          content: Text(l.addedToPlaylist(name)),
          behavior: SnackBarBehavior.floating,
        ),
      );
  }

  await showModalBottomSheet<void>(
    context: context,
    // rootNavigator: über Tab-Leiste und Mini-Player.
    useRootNavigator: true,
    showDragHandle: true,
    builder: (sheetContext) => SafeArea(
      child: FutureBuilder<List<Playlist>>(
        future: rust.playlists(),
        builder: (context, snap) {
          final playlists = snap.data;
          return ListView(
            shrinkWrap: true,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: Text(
                  l.addToPlaylistTitle,
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
              ListTile(
                leading: const Icon(Icons.add),
                title: Text(l.newPlaylist),
                onTap: () async {
                  final name = await _askPlaylistName(sheetContext);
                  if (name == null || name.isEmpty) return;
                  try {
                    await rust.createPlaylist(name: name, songIds: ids);
                    done(name);
                  } catch (e) {
                    messenger.showSnackBar(SnackBar(content: Text('$e')));
                  }
                  if (sheetContext.mounted) Navigator.of(sheetContext).pop();
                },
              ),
              if (snap.hasError)
                Padding(
                  padding: const EdgeInsets.all(16),
                  child: Text('${snap.error}'),
                )
              else if (playlists == null)
                const Padding(
                  padding: EdgeInsets.all(24),
                  child: Center(child: CircularProgressIndicator()),
                )
              else
                for (final p in playlists)
                  ListTile(
                    leading: const Icon(Icons.queue_music),
                    title: Text(p.name),
                    onTap: () async {
                      try {
                        await rust.addToPlaylist(
                          playlistId: p.id,
                          songIds: ids,
                        );
                        done(p.name);
                      } catch (e) {
                        messenger.showSnackBar(SnackBar(content: Text('$e')));
                      }
                      if (sheetContext.mounted) {
                        Navigator.of(sheetContext).pop();
                      }
                    },
                  ),
            ],
          );
        },
      ),
    ),
  );
}

Future<String?> _askPlaylistName(BuildContext context) {
  final l = L.of(context);
  final controller = TextEditingController();
  return showDialog<String>(
    context: context,
    builder: (context) => AlertDialog(
      title: Text(l.newPlaylist),
      content: TextField(
        controller: controller,
        autofocus: true,
        decoration: InputDecoration(hintText: l.playlistName),
        onSubmitted: (v) => Navigator.of(context).pop(v.trim()),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(l.cancel),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(controller.text.trim()),
          child: Text(l.create),
        ),
      ],
    ),
  );
}
