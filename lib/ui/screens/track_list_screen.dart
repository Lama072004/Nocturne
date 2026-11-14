import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../downloads/auto_sync.dart';
import '../../l10n.dart';
import '../../l10n/detail_l10n.dart';
import '../../src/rust/api/models.dart';
import '../../src/rust/api/smart.dart' as smart;
import '../actions.dart';
import '../widgets/async_view.dart';
import '../widgets/track_tile.dart';

/// Download-Knopf für eine Liste von Titeln (Album, Playlist, Favoriten …).
///
/// Zeigt einen Haken, wenn ALLE Titel schon heruntergeladen sind, sonst einen
/// Pfeil. Hört auf `app.downloads`, damit sich das Symbol von selbst ändert,
/// sobald Downloads fertig werden. Wird auch von der Album-Seite benutzt.
class DetailDownloadButton extends StatelessWidget {
  const DetailDownloadButton({super.key, required this.tracks});

  final List<Track> tracks;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final app = AppState.instance;
    return ListenableBuilder(
      listenable: app.downloads,
      builder: (context, _) {
        final all =
            tracks.isNotEmpty && tracks.every((t) => app.downloads.isDownloaded(t.id));
        return IconButton(
          tooltip: all ? l.allDownloaded : l.downloadAll,
          icon: Icon(
            all ? Icons.download_done : Icons.download_for_offline_outlined,
            color: all ? Theme.of(context).colorScheme.primary : null,
          ),
          onPressed: () async {
            final n = await app.downloads.download(tracks, app.settings.value);
            if (!context.mounted) return;
            showToast(context, switch (n) {
              -1 => l.downloadLimitReached,
              0 => l.alreadyDownloaded,
              _ => l.downloadStarted,
            });
          },
        );
      },
    );
  }
}

/// Shuffle-Knopf und großer runder Play-Knopf nebeneinander.
///
/// Gemeinsam für Album-, Künstler- und Titellisten-Seite, damit sich alle
/// gleich anfühlen. [name] erscheint als Herkunft der Warteschlange.
class DetailPlayButtons extends StatelessWidget {
  const DetailPlayButtons({super.key, required this.tracks, required this.name});

  /// Kann leer sein; dann tun die Knöpfe nichts (Player ignoriert leere Listen).
  final List<Track> tracks;
  final String name;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final player = AppState.instance.player;
    final primary = Theme.of(context).colorScheme.primary;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        IconButton(
          tooltip: l.shufflePlay,
          icon: const Icon(Icons.shuffle),
          onPressed: () => player.playShuffled(tracks, name: name),
        ),
        const SizedBox(width: 4),
        // Eigene runde Fläche statt FloatingActionButton: kein Hero-Konflikt
        // zwischen Seiten und feste Größe 56 (>= 48 Berührungsfläche).
        Semantics(
          button: true,
          label: l.play,
          child: Material(
            color: primary,
            shape: const CircleBorder(),
            child: InkWell(
              customBorder: const CircleBorder(),
              onTap: () => player.playContext(tracks, 0, name: name),
              child: SizedBox(
                width: 56,
                height: 56,
                child: Icon(
                  Icons.play_arrow,
                  size: 34,
                  color: Theme.of(context).colorScheme.onPrimary,
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// Allgemeine Seite für "eine Liste von Titeln mit Überschrift".
///
/// Wird für Playlists, Favoriten-Titel, Verlauf, Genres und Smart-Playlists
/// verwendet. Alle brauchen dasselbe: Titel, Anzahl, Abspielen, Zufällig,
/// Herunterladen und die Liste. Eine gemeinsame Seite heißt: gleiches
/// Verhalten überall und nur eine Stelle, an der Fehler behoben werden.
class TrackListScreen extends StatefulWidget {
  const TrackListScreen({
    super.key,
    required this.title,
    required this.load,
    this.pinPlaylistId,
  });

  final String title;

  /// ID einer Server-Playlist. Ist sie gesetzt, zeigt die Seite einen
  /// Schalter "Offline aktuell halten" (siehe `AutoSync`).
  final String? pinPlaylistId;

  /// Lädt die Titel. Als Funktion, damit "Erneut versuchen" neu laden kann.
  final Future<List<Track>> Function() load;

  @override
  State<TrackListScreen> createState() => _TrackListScreenState();
}

class _TrackListScreenState extends State<TrackListScreen> {
  // Der Zustand wird selbst verwaltet (statt FutureBuilder), damit beim
  // Herunterziehen die alte Liste stehen bleibt, bis die neue da ist.
  List<Track>? _tracks;
  Object? _error;
  bool _pinned = false;

  @override
  void initState() {
    super.initState();
    _reload();
    final id = widget.pinPlaylistId;
    if (id != null) {
      // Ist diese Playlist schon vorgemerkt?
      smart
          .pinnedPlaylists()
          .then((list) {
            if (mounted) setState(() => _pinned = list.any((p) => p.id == id));
          })
          .catchError((_) {});
    }
  }

  /// Lädt die Titel neu. Gibt das Future zurück, damit der Ziehen-zum-
  /// Aktualisieren-Kreis genau so lange dreht, wie das Laden dauert.
  Future<void> _reload() async {
    try {
      final tracks = await widget.load();
      if (!mounted) return;
      setState(() {
        _tracks = tracks;
        _error = null;
      });
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e;
      });
    }
  }

  Future<void> _togglePinned() async {
    final l = L.of(context);
    final next = !_pinned;
    await smart.setPlaylistPinned(
      id: widget.pinPlaylistId!,
      name: widget.title,
      pinned: next,
    );
    if (!mounted) return;
    setState(() => _pinned = next);
    if (next) {
      showToast(context, l.keepOfflineOn);
      AutoSync.run();
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final tracks = _tracks;

    return Scaffold(
      appBar: AppBar(
        title: Text(widget.title),
        actions: [
          if (widget.pinPlaylistId != null)
            IconButton(
              tooltip: l.keepOffline,
              icon: Icon(_pinned ? Icons.offline_pin : Icons.offline_pin_outlined),
              color: _pinned ? theme.colorScheme.primary : null,
              onPressed: _togglePinned,
            ),
        ],
      ),
      body: _body(context, l, theme, tracks),
    );
  }

  Widget _body(BuildContext context, L l, ThemeData theme, List<Track>? tracks) {
    if (tracks == null) {
      if (_error != null) {
        // Fehler beim ersten Laden: Meldung plus nächster Schritt.
        return Center(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Padding(
                padding: const EdgeInsets.all(24),
                child: Text('$_error', textAlign: TextAlign.center),
              ),
              FilledButton(
                onPressed: () {
                  setState(() => _error = null);
                  _reload();
                },
                child: Text(l.retry),
              ),
            ],
          ),
        );
      }
      return const Center(child: CircularProgressIndicator());
    }

    return RefreshIndicator(
      onRefresh: _reload,
      child: tracks.isEmpty
          // ListView, damit "Nach unten ziehen" auch bei leerer Liste geht.
          ? ListView(
              physics: const AlwaysScrollableScrollPhysics(),
              children: [
                SizedBox(
                  height: MediaQuery.sizeOf(context).height * 0.6,
                  child: EmptyHint(
                    icon: Icons.music_off_outlined,
                    text: l.empty,
                    detail: l.refreshHint,
                  ),
                ),
              ],
            )
          : _list(context, l, theme, tracks),
    );
  }

  Widget _list(BuildContext context, L l, ThemeData theme, List<Track> tracks) {
    final app = AppState.instance;
    final secs = tracks.fold<int>(0, (s, t) => s + (t.durationSecs ?? 0));
    return CustomScrollView(
      // Immer scrollbar, sonst geht Ziehen-zum-Aktualisieren bei kurzen Listen nicht.
      physics: const AlwaysScrollableScrollPhysics(),
      slivers: [
        SliverToBoxAdapter(
          child: Padding(
            padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    [l.songs(tracks.length), if (secs > 0) l.totalLength(secs)].join(' · '),
                    style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                  ),
                ),
                DetailDownloadButton(tracks: tracks),
                DetailPlayButtons(tracks: tracks, name: widget.title),
              ],
            ),
          ),
        ),
        // Feste Zeilenhöhe: Flutter muss nicht jede Zeile messen, das
        // Scrollen bleibt auch bei tausenden Titeln flüssig.
        SliverFixedExtentList.builder(
          itemExtent: kTrackRowHeight,
          itemCount: tracks.length,
          itemBuilder: (context, i) => TrackTile(
            track: tracks[i],
            onTap: () => app.player.playContext(tracks, i, name: widget.title),
          ),
        ),
        // Abstand unten, damit der letzte Titel nicht am Rand / unter dem
        // Mini-Player klebt.
        const SliverToBoxAdapter(child: SizedBox(height: 96)),
      ],
    );
  }
}
