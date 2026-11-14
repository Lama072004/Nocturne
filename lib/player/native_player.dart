import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Ein Eintrag, wie ihn der native Player (Media3/ExoPlayer) braucht.
///
/// `entryId` ist die eindeutige ID des Warteschlangen-Eintrags (aus Rust),
/// nicht die Song-ID. Grund: Derselbe Song darf mehrfach in der Warteschlange
/// stehen, und wir müssen beim Titelwechsel genau wissen, welcher Eintrag
/// gemeint ist.
class NativeQueueItem {
  const NativeQueueItem({
    required this.entryId,
    required this.songId,
    required this.title,
    this.artist,
    this.album,
    this.coverId,
    this.trackGain,
    this.albumGain,
    this.trackPeak,
    this.albumPeak,
  });

  final String entryId;

  /// Song-ID des Servers. Der Android-Dienst holt sich damit selbst die
  /// Abspiel-Adresse aus Rust und findet heruntergeladene Titel wieder.
  ///
  /// WARUM schicken wir keine fertige Adresse mit? Sie enthält die
  /// Anmeldedaten. Was im MediaItem steht, können andere Apps über die
  /// Mediensitzung lesen (z. B. Auto-Apps). Die ID allein verrät nichts.
  final String songId;
  final String title;
  final String? artist;
  final String? album;

  /// ID des Covers. Android macht daraus eine eigene `content://`-Adresse
  /// (ArtworkProvider), ebenfalls ohne Anmeldedaten.
  final String? coverId;

  /// ReplayGain-Werte des Titels (dB bzw. Spitzenpegel), `null` = unbekannt.
  /// Der Android-Dienst gibt sie beim Titelwechsel an die Effektkette.
  final double? trackGain;
  final double? albumGain;
  final double? trackPeak;
  final double? albumPeak;

  Map<String, Object?> toMap() => {
    'entryId': entryId,
    'songId': songId,
    'title': title,
    'artist': artist,
    'album': album,
    'coverId': coverId,
    'trackGain': trackGain,
    'albumGain': albumGain,
    'trackPeak': trackPeak,
    'albumPeak': albumPeak,
  };
}

/// Zustand des nativen Players als unveränderlicher Schnappschuss.
@immutable
class NativePlayerState {
  const NativePlayerState({
    this.isPlaying = false,
    this.isBuffering = false,
    this.index = -1,
    this.entryId,
    this.positionMs = 0,
    this.durationMs,
    this.timestampMs = 0,
  });

  final bool isPlaying;
  final bool isBuffering;
  final int index;
  final String? entryId;
  final int positionMs;
  final int? durationMs;

  /// Wann die Position gemessen wurde (Unix-ms).
  final int timestampMs;

  /// Aktuelle Position, hochgerechnet aus dem letzten Schnappschuss.
  ///
  /// WARUM hochrechnen statt ständig nachfragen? Jede Abfrage über den
  /// Plattform-Kanal weckt die native Seite auf. Die UI fragt diese Methode
  /// nur ab, solange der Fortschrittsbalken sichtbar ist – bei
  /// ausgeschaltetem Bildschirm passiert gar nichts. Das spart Akku.
  Duration get currentPosition {
    var pos = positionMs;
    if (isPlaying && !isBuffering) {
      pos += DateTime.now().millisecondsSinceEpoch - timestampMs;
    }
    final dur = durationMs;
    if (dur != null && pos > dur) pos = dur;
    return Duration(milliseconds: pos);
  }

  static NativePlayerState fromMap(Map<Object?, Object?> m) =>
      NativePlayerState(
        isPlaying: m['isPlaying'] as bool? ?? false,
        isBuffering: m['isBuffering'] as bool? ?? false,
        index: m['index'] as int? ?? -1,
        entryId: m['entryId'] as String?,
        positionMs: (m['positionMs'] as num?)?.toInt() ?? 0,
        durationMs: (m['durationMs'] as num?)?.toInt(),
        timestampMs: (m['timestampMs'] as num?)?.toInt() ?? 0,
      );
}

/// Dünne Dart-Hülle um die Kanäle aus `PlayerBridge.kt`.
///
/// Diese Klasse kennt KEINE Warteschlangen-Logik. Sie reicht nur Befehle an
/// Media3 durch. Die Logik (Spotify-Warteschlange) lebt im Rust-Kern, siehe
/// `PlayerController`.
class NativePlayer {
  static const _methods = MethodChannel('musicplayer/player');
  static const _events = EventChannel('musicplayer/player_events');

  /// Strom aller Zustandsänderungen. `broadcast`, weil mehrere Widgets
  /// (Mini-Player, Vollbild-Player) gleichzeitig zuhören dürfen.
  late final Stream<NativePlayerState> states = _events
      .receiveBroadcastStream()
      .map((e) => NativePlayerState.fromMap(e as Map<Object?, Object?>))
      .asBroadcastStream();

  /// Ersetzt die komplette Playlist und startet bei [index].
  Future<void> setQueue(
    List<NativeQueueItem> items,
    int index, {
    bool play = true,
  }) => _methods.invokeMethod('setQueue', {
    'items': items.map((e) => e.toMap()).toList(),
    'index': index,
    'play': play,
  });

  /// Aktualisiert die Playlist, ohne den laufenden Titel zu unterbrechen.
  Future<void> updateQueue(List<NativeQueueItem> items, int index) =>
      _methods.invokeMethod('updateQueue', {
        'items': items.map((e) => e.toMap()).toList(),
        'index': index,
      });

  Future<void> play() => _methods.invokeMethod('play');
  Future<void> pause() => _methods.invokeMethod('pause');
  Future<void> next() => _methods.invokeMethod('next');
  Future<void> previous() => _methods.invokeMethod('previous');
  Future<void> seekTo(Duration pos) =>
      _methods.invokeMethod('seekTo', {'positionMs': pos.inMilliseconds});
  Future<void> skipTo(int index) =>
      _methods.invokeMethod('skipTo', {'index': index});

  /// [mode] ist "off", "all" oder "one".
  Future<void> setRepeatMode(String mode) =>
      _methods.invokeMethod('setRepeatMode', {'mode': mode});

  /// Sleep-Timer: > 0 = Minuten, -1 = bis zum Ende des Titels, 0 = aus.
  /// Der Timer läuft im Android-Dienst, damit er auch auslöst, wenn die
  /// Oberfläche längst geschlossen ist.
  Future<void> setSleepTimer(int minutes) =>
      _methods.invokeMethod('setSleepTimer', {'minutes': minutes});

  /// Meldet dem Wiedergabe-Dienst, dass sich die Klang-Einstellungen
  /// geändert haben. Er schaltet dann Hardware-Offload passend um.
  Future<void> dspChanged() => _methods.invokeMethod('dspChanged');

  /// Überblenden zwischen Titeln: Dauer in Sekunden, 0 = aus. Gemischt wird
  /// im Android-Dienst direkt im Audio-Pfad (siehe CrossfadeProcessor.kt).
  Future<void> setCrossfade(int seconds) =>
      _methods.invokeMethod('setCrossfade', {'seconds': seconds});
}
