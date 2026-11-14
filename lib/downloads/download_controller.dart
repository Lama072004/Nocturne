import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

import '../src/rust/api/downloads.dart' as rust_dl;
import '../src/rust/api/library.dart' as rust;
import '../src/rust/api/models.dart';
import '../src/rust/api/settings.dart';

/// Zustand eines einzelnen Downloads, wie ihn Android meldet.
enum DownloadState { queued, downloading, completed, failed, removing }

@immutable
class DownloadInfo {
  const DownloadInfo({required this.state, this.percent, this.bytes = 0});

  final DownloadState state;

  /// 0–100, `null` solange die Gesamtgröße unbekannt ist (kommt bei
  /// umgewandelten Streams vor: Der Server kennt die Größe vorab nicht).
  final double? percent;
  final int bytes;
}

/// Hält die beiden Hälften eines Downloads zusammen:
///
///   Android/Media3 : lädt und speichert die AUDIODATEI (im Hintergrund)
///   Rust-Datenbank : merkt sich die METADATEN (Titel, Album, Cover …)
///
/// Die UI fragt nur diese Klasse.
class DownloadController extends ChangeNotifier {
  static const _methods = MethodChannel('musicplayer/downloads');
  static const _events = EventChannel('musicplayer/download_events');

  /// Heruntergeladene (oder vorgemerkte) Titel, neueste zuerst.
  List<Track> tracks = const [];

  /// Status je Song-ID.
  Map<String, DownloadInfo> status = const {};

  StreamSubscription<dynamic>? _sub;

  /// Gesamtgröße aller Downloads in Bytes.
  int get totalBytes => status.values.fold(0, (sum, d) => sum + d.bytes);

  bool isDownloaded(String songId) =>
      status[songId]?.state == DownloadState.completed;
  bool isKnown(String songId) => status.containsKey(songId);

  bool get hasActive => status.values.any(
    (d) =>
        d.state == DownloadState.queued || d.state == DownloadState.downloading,
  );

  Future<void> init(AppSettings settings) async {
    await configure(settings);
    // Android meldet nur, DASS sich etwas geändert hat. Die Details holen
    // wir dann gesammelt mit einer einzigen Abfrage.
    _sub ??= _events.receiveBroadcastStream().listen((_) => refresh());
    await refresh();
  }

  /// Gibt die download-relevanten Einstellungen an Android weiter. Dort
  /// werden sie zusätzlich gespeichert, weil der Download-Dienst auch ohne
  /// laufende Oberfläche weiterarbeitet.
  Future<void> configure(AppSettings s) => _methods.invokeMethod('configure', {
    'wifiOnly': s.downloadWifiOnly,
    'sdCard': s.downloadsOnSdCard,
    'streamCacheMb': s.streamCacheMb,
  });

  /// Lädt Metadaten (Rust) und Status (Android) neu.
  Future<void> refresh() async {
    try {
      final meta = await rust_dl.downloadedTracks();
      final raw =
          await _methods.invokeListMethod<Map<Object?, Object?>>('list') ??
          const [];
      status = {
        for (final m in raw)
          m['id'] as String: DownloadInfo(
            state: DownloadState.values.byName(m['state'] as String),
            percent: (m['percent'] as num?)?.toDouble(),
            bytes: (m['bytes'] as num?)?.toInt() ?? 0,
          ),
      };
      tracks = meta;
      notifyListeners();
    } catch (_) {
      // Ein Fehler beim Auffrischen darf die App nicht stören; beim
      // nächsten Ereignis wird es erneut versucht.
    }
  }

  /// Startet Downloads für [list]. Bereits bekannte Titel werden übersprungen.
  ///
  /// Gibt die Anzahl der neu gestarteten Downloads zurück, oder -1, wenn das
  /// Speicherlimit aus den Einstellungen schon erreicht ist.
  Future<int> download(List<Track> list, AppSettings settings) async {
    final limitBytes = settings.downloadLimitMb * 1024 * 1024;
    final hasLimit = settings.downloadLimitMb > 0;
    // Bereits belegter Platz plus der GESCHÄTZTE Platz der Titel, die in
    // diesem Durchgang dazukommen. Ohne die Schätzung würde ein ganzes
    // Album auf einmal starten, obwohl nur noch Platz für zwei Titel ist –
    // die laufenden Downloads zählen ja erst mit, wenn sie Daten haben.
    var used = totalBytes;
    if (hasLimit && used >= limitBytes) return -1;

    var started = 0;
    var limitHit = false;
    for (final t in list) {
      if (isKnown(t.id)) continue;
      // Adresse in der Download-Qualität (aus Rust, mit Anmeldung).
      final url = rust.downloadUrl(songId: t.id);
      if (url == null) continue;
      final estimate = _estimateBytes(t, settings.qualityDownload);
      if (hasLimit && used + estimate > limitBytes) {
        limitHit = true;
        break;
      }
      // Erst die Metadaten sichern, dann den Download starten: Würde die
      // App dazwischen beendet, gäbe es sonst eine Datei ohne Namen.
      await rust_dl.saveDownload(track: t);
      try {
        await _methods.invokeMethod('start', {'id': t.id, 'url': url});
      } catch (_) {
        // Android konnte den Download nicht annehmen -> den Merkzettel
        // wieder entfernen, sonst stünde ein Titel in der Liste, der nie lädt.
        await rust_dl.removeDownload(trackId: t.id);
        break;
      }
      used += estimate;
      started++;
    }
    await refresh();
    return (started == 0 && limitHit) ? -1 : started;
  }

  /// Grobe Größe eines Titels in der gewählten Download-Qualität.
  /// Bei "Original" kennen wir die Bitrate nicht und rechnen mit 900 kbit/s
  /// (übliche FLAC-Größe) – lieber etwas zu vorsichtig als das Limit reißen.
  int _estimateBytes(Track t, StreamQuality q) {
    final seconds = t.durationSecs ?? 240;
    final kbps = q.format == AudioFormat.original ? 900 : q.bitrateKbps;
    return seconds * kbps * 1000 ~/ 8;
  }

  Future<void> remove(String songId) async {
    await _methods.invokeMethod('remove', {'id': songId});
    await rust_dl.removeDownload(trackId: songId);
    await refresh();
  }

  /// Ist eine SD-Karte eingelegt, auf die die App schreiben darf?
  Future<bool> sdCardAvailable() async {
    try {
      final info = await _methods.invokeMapMethod<String, dynamic>(
        'storageInfo',
      );
      return info?['sdCardAvailable'] == true;
    } catch (_) {
      return false;
    }
  }

  Future<void> removeAll() async {
    await _methods.invokeMethod('removeAll');
    for (final t in tracks) {
      await rust_dl.removeDownload(trackId: t.id);
    }
    await refresh();
  }
}
