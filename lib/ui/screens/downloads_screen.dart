import 'dart:async';

import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../downloads/download_controller.dart';
import '../../l10n.dart';
import '../actions.dart';
import '../widgets/async_view.dart';
import '../widgets/track_tile.dart';

/// "1,2 GB" / "340 MB" aus Bytes.
String formatBytes(int bytes) {
  const mb = 1024 * 1024;
  if (bytes >= 1024 * mb) {
    return '${(bytes / (1024 * mb)).toStringAsFixed(1)} GB';
  }
  return '${(bytes / mb).round()} MB';
}

/// Downloads-Tab: Speicherübersicht, laufende Downloads und alle
/// heruntergeladenen Titel. Funktioniert komplett ohne Netz, weil die
/// Metadaten in der lokalen Datenbank liegen.
class DownloadsScreen extends StatefulWidget {
  const DownloadsScreen({super.key});

  @override
  State<DownloadsScreen> createState() => _DownloadsScreenState();
}

class _DownloadsScreenState extends State<DownloadsScreen> {
  final _downloads = AppState.instance.downloads;
  Timer? _poll;

  @override
  void initState() {
    super.initState();
    _downloads.addListener(_syncPolling);
    _downloads.refresh();
  }

  /// Der Fortschritt in Prozent kommt nicht als Ereignis (das wären viele
  /// Aufweck-Vorgänge pro Sekunde). Stattdessen fragen wir einmal pro
  /// Sekunde nach – aber NUR, solange diese Seite offen ist UND wirklich
  /// etwas lädt. Sonst läuft kein Timer.
  void _syncPolling() {
    if (_downloads.hasActive && _poll == null) {
      _poll = Timer.periodic(
        const Duration(seconds: 1),
        (_) => _downloads.refresh(),
      );
    } else if (!_downloads.hasActive && _poll != null) {
      _poll!.cancel();
      _poll = null;
    }
  }

  @override
  void dispose() {
    _downloads.removeListener(_syncPolling);
    _poll?.cancel();
    super.dispose();
  }

  Future<void> _confirmRemoveAll() async {
    final l = L.of(context);
    final ok = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.removeAllDownloads),
        content: Text(l.removeAllDownloadsConfirm),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: Text(l.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(l.delete),
          ),
        ],
      ),
    );
    if (ok == true) await _downloads.removeAll();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final app = AppState.instance;

    return ListenableBuilder(
      listenable: _downloads,
      builder: (context, _) {
        final tracks = _downloads.tracks;
        final limitMb = app.settings.value.downloadLimitMb;
        return Scaffold(
          appBar: AppBar(
            title: Text(l.downloads),
            actions: [
              if (tracks.isNotEmpty)
                IconButton(
                  tooltip: l.removeAllDownloads,
                  icon: const Icon(Icons.delete_sweep_outlined),
                  onPressed: _confirmRemoveAll,
                ),
            ],
          ),
          body: tracks.isEmpty
              ? EmptyHint(
                  icon: Icons.download_outlined,
                  text: l.noDownloads,
                  detail: l.noDownloadsHint,
                )
              : ListView.builder(
                  itemCount: tracks.length + 1,
                  itemBuilder: (context, i) {
                    if (i == 0) {
                      // Speicherübersicht, bei gesetztem Limit mit Balken.
                      return Padding(
                        padding: const EdgeInsets.fromLTRB(16, 4, 16, 12),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              l.storageUsed(
                                formatBytes(_downloads.totalBytes),
                                tracks.length,
                              ),
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: theme.hintColor,
                              ),
                            ),
                            if (limitMb > 0) ...[
                              const SizedBox(height: 6),
                              LinearProgressIndicator(
                                value:
                                    (_downloads.totalBytes /
                                            (limitMb * 1024 * 1024))
                                        .clamp(0.0, 1.0),
                              ),
                            ],
                          ],
                        ),
                      );
                    }
                    final track = tracks[i - 1];
                    final info = _downloads.status[track.id];
                    return TrackTile(
                      track: track,
                      // Tippen spielt alle Downloads ab diesem Titel. Die
                      // Dateien kommen aus dem Cache, ganz ohne Netz.
                      onTap: () => app.player.playContext(
                        tracks,
                        i - 1,
                        name: l.downloads,
                      ),
                      trailing: _StatusIcon(
                        info: info,
                        onRemove: () async {
                          await _downloads.remove(track.id);
                          if (context.mounted) {
                            showToast(context, l.downloadRemoved);
                          }
                        },
                      ),
                    );
                  },
                ),
        );
      },
    );
  }
}

/// Rechts in der Zeile: Fortschritt, Fehler oder der Knopf zum Löschen.
class _StatusIcon extends StatelessWidget {
  const _StatusIcon({required this.info, required this.onRemove});

  final DownloadInfo? info;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final state = info?.state;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (state == DownloadState.downloading)
          SizedBox(
            width: 20,
            height: 20,
            child: CircularProgressIndicator(
              strokeWidth: 2,
              // Ohne bekannte Größe dreht sich der Kreis einfach.
              value: info?.percent == null ? null : info!.percent! / 100,
            ),
          )
        else if (state == DownloadState.queued)
          Tooltip(
            message: l.waiting,
            child: Icon(Icons.schedule, color: theme.hintColor),
          )
        else if (state == DownloadState.failed)
          Tooltip(
            message: l.failed,
            child: Icon(Icons.error_outline, color: theme.colorScheme.error),
          ),
        IconButton(
          tooltip: l.delete,
          icon: const Icon(Icons.delete_outline),
          onPressed: onRemove,
        ),
      ],
    );
  }
}
