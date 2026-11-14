import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../downloads/download_controller.dart';
import '../../l10n.dart';
import '../../src/rust/api/models.dart';
import '../../src/rust/api/settings.dart';
import '../actions.dart';
import 'context_menu.dart';
import 'cover.dart';
import 'swipe_row.dart';

/// "3:45" aus Sekunden.
String formatDuration(int? secs) {
  if (secs == null) return '';
  final m = secs ~/ 60;
  final s = (secs % 60).toString().padLeft(2, '0');
  return '$m:$s';
}

/// Feste Höhe jeder Titel-Zeile.
///
/// WARUM fest? Kennt Flutter die Höhe jeder Zeile vorab (`itemExtent` in
/// Listen), muss es beim Scrollen und beim Springen an eine Stelle nichts
/// ausmessen. Das ist der wichtigste einzelne Hebel für flüssiges Scrollen
/// in Sammlungen mit tausenden Titeln.
const double kTrackRowHeight = 64;

/// Eine Titel-Zeile, wie sie überall in der App vorkommt (Album, Playlist,
/// Suche, Warteschlange, Downloads).
///
/// Bedienung:
///  - Tippen: abspielen
///  - Wischen: einstellbare Abkürzungen (siehe Einstellungen › Wischgesten)
///  - Drei Punkte ODER langes Drücken: Menü mit allen Aktionen
///
/// Zustand, den man der Zeile ansieht:
///  - Akzentfarbe: läuft gerade
///  - Herz: Favorit
///  - Haken: heruntergeladen, Kreis: lädt gerade
///  - halb durchsichtig: offline nicht verfügbar
class TrackTile extends StatelessWidget {
  const TrackTile({
    super.key,
    required this.track,
    required this.onTap,
    this.swipes,
    this.showCover = true,
    this.queueIndex,
    this.trailing,
    this.showDuration = true,
  });

  final Track track;
  final VoidCallback onTap;

  /// Belegung der Gesten. Standard: die für Titel-Zeilen aus den Einstellungen.
  final SwipeSet? swipes;

  /// Im Album ist das Cover überall gleich -> dort stattdessen die Titelnummer.
  final bool showCover;

  /// Position in der Warteschlange (nur dort gesetzt).
  final int? queueIndex;

  /// Zusätzliches Element rechts, VOR dem Drei-Punkte-Knopf (z. B. der
  /// Ziehgriff in der Warteschlange).
  final Widget? trailing;

  final bool showDuration;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final app = AppState.instance;

    return ValueListenableBuilder(
      valueListenable: app.settings,
      builder: (context, settings, _) => SwipeRow(
        actions: swipes ?? settings.swipes.trackRows,
        onAction: (a) => runSwipeAction(
          context,
          a,
          tracks: () async => [track],
          queueIndex: queueIndex,
          favorite: (
            kind: StarKind.track,
            id: track.id,
            starred: app.favorites.isTrackStarred(track),
          ),
        ),
        child: ListenableBuilder(
          // Hört auf alles, was das Aussehen der Zeile ändert. Neu gebaut
          // wird dabei nur diese eine Zeile, nicht die ganze Liste.
          listenable: Listenable.merge([
            app.player,
            app.favorites,
            app.downloads,
            app.offline,
          ]),
          builder: (context, _) {
            final isCurrent = queueIndex == null && app.player.currentTrack?.id == track.id;
            final download = app.downloads.status[track.id];
            final downloaded = download?.state == DownloadState.completed;
            // Offline lassen sich nur heruntergeladene Titel abspielen.
            final unavailable = app.offline.value && !downloaded;

            void openMenu() => showTrackMenu(context, track, queueIndex: queueIndex);

            final row = InkWell(
              onTap: unavailable
                  ? () => showToast(context, L.of(context).offlineNotAvailable)
                  : onTap,
              onLongPress: openMenu,
              child: SizedBox(
                height: kTrackRowHeight,
                child: Padding(
                  padding: const EdgeInsets.only(left: 16),
                  child: Row(
                    children: [
                      if (showCover)
                        Cover(coverId: track.coverArtId, size: 48)
                      else
                        SizedBox(
                          width: 32,
                          child: Text(
                            '${track.trackNumber ?? ''}',
                            textAlign: TextAlign.center,
                            style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                          ),
                        ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          mainAxisAlignment: MainAxisAlignment.center,
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              track.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.bodyLarge?.copyWith(
                                color: isCurrent ? theme.colorScheme.primary : null,
                                fontWeight: isCurrent ? FontWeight.w600 : null,
                              ),
                            ),
                            const SizedBox(height: 2),
                            Row(
                              children: [
                                _StateIcons(
                                  starred: app.favorites.isTrackStarred(track),
                                  download: download,
                                ),
                                Expanded(
                                  child: Text(
                                    track.artist ?? '',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: theme.textTheme.bodyMedium?.copyWith(
                                      color: theme.hintColor,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ],
                        ),
                      ),
                      if (showDuration && track.durationSecs != null)
                        Padding(
                          padding: const EdgeInsets.only(left: 8),
                          child: Text(
                            formatDuration(track.durationSecs),
                            style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
                          ),
                        ),
                      if (trailing != null) trailing!,
                      MoreButton(onPressed: openMenu),
                    ],
                  ),
                ),
              ),
            );
            return unavailable ? Opacity(opacity: 0.45, child: row) : row;
          },
        ),
      ),
    );
  }
}

/// Die kleinen Zustands-Symbole vor dem Künstlernamen.
class _StateIcons extends StatelessWidget {
  const _StateIcons({required this.starred, required this.download});

  final bool starred;
  final DownloadInfo? download;

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.primary;
    final state = download?.state;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (state == DownloadState.completed)
          Padding(
            padding: const EdgeInsets.only(right: 4),
            child: Icon(Icons.download_done, size: 14, color: color),
          )
        else if (state == DownloadState.downloading || state == DownloadState.queued)
          Padding(
            padding: const EdgeInsets.only(right: 6),
            child: SizedBox(
              width: 11,
              height: 11,
              child: CircularProgressIndicator(
                strokeWidth: 1.6,
                // Ohne bekannten Fortschritt (wartet noch) dreht sich der Kreis.
                value: download?.percent == null ? null : download!.percent! / 100,
              ),
            ),
          ),
        if (starred)
          Padding(
            padding: const EdgeInsets.only(right: 4),
            child: Icon(Icons.favorite, size: 13, color: color),
          ),
      ],
    );
  }
}
