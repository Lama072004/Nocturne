import 'dart:async';

import 'package:flutter/foundation.dart';

import '../prefs.dart';
import '../src/rust/api/models.dart';
import '../src/rust/api/queue.dart';
import 'native_player.dart';

/// Merkzettel-Schlüssel für die Crossfade-Dauer (Sekunden als Text).
const kCrossfadePref = 'playback.crossfade';

/// Verbindet die zwei Hälften der Wiedergabe:
///
///   Rust-Kern  (weiß, WAS als Nächstes kommt  -> Spotify-Warteschlange)
///   Media3     (spielt tatsächlich ab          -> PlaybackService.kt)
///
/// Ablauf bei jeder Änderung:
///   1. Die UI ruft hier eine Methode auf (z. B. [addToQueue]).
///   2. Rust ändert die Warteschlange und liefert einen Schnappschuss.
///   3. Wir schicken die flache Liste an Media3.
///
/// Und umgekehrt: Wechselt Media3 von selbst den Titel (Titelende,
/// Benachrichtigung, Kopfhörertaste, Android Auto), melden wir das an Rust,
/// damit beide Seiten wieder denselben Stand haben.
///
/// # Reihenfolge ist wichtig
/// Alle Änderungen an der Warteschlange laufen NACHEINANDER durch [_serial].
/// Ohne das könnten sich zwei Vorgänge überholen – etwa ein automatischer
/// Titelwechsel und ein gleichzeitiges "Zur Warteschlange": Der langsamere
/// würde den Stand des schnelleren überschreiben, und die Wiedergabe spränge
/// im schlimmsten Fall auf den vorherigen Titel zurück.
///
/// Die UI hört per [ChangeNotifier] zu. Das reicht hier völlig; ein
/// zusätzliches State-Management-Paket würde nur Abhängigkeiten bringen.
class PlayerController extends ChangeNotifier {
  final NativePlayer _native = NativePlayer();
  StreamSubscription<NativePlayerState>? _sub;

  /// Aktueller Stand der Warteschlange aus Rust.
  QueueSnapshot? snapshot;

  /// Aktueller Stand des nativen Players.
  NativePlayerState state = const NativePlayerState();

  Track? get currentTrack => snapshot?.current?.track;
  bool get hasTrack => snapshot?.current != null;
  bool get shuffle => snapshot?.shuffle ?? false;
  RepeatMode get repeat => snapshot?.repeat ?? RepeatMode.off;

  /// Wann der Sleep-Timer auslöst (nur zur Anzeige; der echte Timer läuft
  /// im Android-Dienst). `null` = kein Timer.
  DateTime? sleepUntil;

  /// `true` = Sleep-Timer steht auf "bis zum Ende des Titels".
  bool sleepAtTrackEnd = false;

  // HÖRZEIT (Verlauf, Statistik, Scrobbling) wird NICHT hier gezählt,
  // sondern im Android-Dienst (PlaybackService, Abschnitt "Hörzeit"). Die
  // Oberfläche kann beendet sein, während die Musik weiterläuft – dann
  // fehlten die Titel im Verlauf. Zählten beide, stünde jeder Titel doppelt
  // darin.

  /// Der vorige Zustand (um "Alle wiederholen" zu erkennen, siehe
  /// [_looksLikeRepeatWrap]).
  NativePlayerState? _lastState;
  bool _lastWasPlaying = false;

  /// Ende der Kette aus nacheinander laufenden Vorgängen (siehe [_serial]).
  Future<void> _chain = Future.value();

  void init() {
    _sub ??= _native.states.listen(
      _onNativeState,
      // Ein Fehler im Ereignis-Kanal darf das Zuhören nicht beenden.
      onError: (_) {},
    );
    // Stand aus Rust holen: Läuft schon Musik (App wurde nur neu geöffnet),
    // zeigt die UI sofort den richtigen Titel und die Warteschlange.
    _serial(() async {
      snapshot = await queueSnapshot();
      notifyListeners();
    });
    // Crossfade-Dauer an den Dienst geben. Er merkt sie sich zwar selbst
    // (für Wiedergabe ohne Oberfläche), aber die Einstellung in der App ist
    // die Wahrheit – z. B. nach "App-Daten wiederhergestellt".
    Prefs.instance.load(kCrossfadePref).then((v) {
      _native.setCrossfade(int.tryParse(v ?? '') ?? 0).catchError((_) {});
    });
  }

  /// Überblenden zwischen Titeln in Sekunden (0 = aus). Speichert die Wahl
  /// und gibt sie an den Wiedergabe-Dienst weiter.
  Future<void> setCrossfade(int seconds) async {
    Prefs.instance.set(kCrossfadePref, '$seconds');
    await _native.setCrossfade(seconds);
  }

  /// Aktuell gewählte Crossfade-Dauer in Sekunden.
  int get crossfadeSeconds =>
      int.tryParse(Prefs.instance.get(kCrossfadePref) ?? '') ?? 0;

  @override
  void dispose() {
    _sub?.cancel();
    super.dispose();
  }

  /// Hängt [op] hinten an die Kette: Er startet erst, wenn alle vorherigen
  /// Vorgänge fertig sind. Fehler eines Vorgangs halten die Kette nicht an.
  Future<T> _serial<T>(Future<T> Function() op) {
    final result = _chain.then((_) => op());
    _chain = result.then<void>((_) {}, onError: (_) {});
    return result;
  }

  void _onNativeState(NativePlayerState s) {
    final previous = _lastState;
    _lastState = s;
    state = s;

    // Sleep-Timer ist abgelaufen bzw. hat pausiert -> Anzeige zurücksetzen.
    if (!s.isPlaying &&
        sleepUntil != null &&
        DateTime.now().isAfter(sleepUntil!)) {
      sleepUntil = null;
    }
    if (!s.isPlaying && sleepAtTrackEnd && _lastWasPlaying) {
      sleepAtTrackEnd = false;
    }
    _lastWasPlaying = s.isPlaying;
    notifyListeners();

    final id = s.entryId;
    if (id == null) return;
    // Media3 spielt einen anderen Eintrag, als Rust für "aktuell" hält
    // -> Rust nachziehen. Das läuft in der Kette, damit es sich nicht mit
    // einer gleichzeitigen Nutzer-Aktion überholt.
    _serial(() async {
      if (id == snapshot?.current?.entryId) return;
      // Kennt unser Schnappschuss den Eintrag gar nicht, hat jemand anderes
      // die Warteschlange gesetzt: Android Auto, ein Widget, der
      // Sprachassistent oder der Dienst selbst, während die Oberfläche
      // geschlossen war. Rust kennt die neue Liste schon (sie wurde dort
      // gebaut) -> einfach den Stand holen statt "nachzuziehen".
      final known = snapshot?.nativeItems.any((i) => i.entryId == id) ?? false;
      if (!known) {
        snapshot = await queueSnapshot();
        if (snapshot?.current?.entryId == id) {
          notifyListeners();
          return;
        }
      }
      final snap = await queueOnNativeTransition(
        entryId: id,
        wrapped: _looksLikeRepeatWrap(previous, s),
      );
      snapshot = snap;
      if (snap.needsNativeUpdate) await _updateNative(snap);
      notifyListeners();
    });
  }

  /// Ist Media3 am ENDE der Liste von selbst wieder nach vorne gesprungen
  /// ("Alle wiederholen")?
  ///
  /// Media3 meldet uns nur den neuen Eintrag, nicht den Grund. Wir erkennen
  /// den Umbruch daran, dass der letzte Eintrag bis (fast) zum Schluss lief
  /// und danach der erste kommt. Drückt der Nutzer dagegen "Zurück" bis zum
  /// ersten Titel, lief der letzte nicht zu Ende – dann ist es kein Umbruch,
  /// und eingereihte Titel wandern zurück in die Warteschlange.
  bool _looksLikeRepeatWrap(NativePlayerState? before, NativePlayerState now) {
    final snap = snapshot;
    if (before == null || snap == null || repeat != RepeatMode.all) {
      return false;
    }
    final lastIndex = snap.nativeItems.length - 1;
    if (now.index != 0 || before.index != lastIndex || lastIndex == 0) {
      return false;
    }
    final duration = before.durationMs;
    if (duration == null) return false;
    return before.currentPosition.inMilliseconds >= duration - 3000;
  }

  List<NativeQueueItem> _items(QueueSnapshot s) => [
    for (final i in s.nativeItems)
      NativeQueueItem(
        entryId: i.entryId,
        songId: i.songId,
        title: i.title,
        artist: i.artist,
        album: i.album,
        coverId: i.coverId,
        trackGain: i.trackGainDb,
        albumGain: i.albumGainDb,
        trackPeak: i.trackPeak,
        albumPeak: i.albumPeak,
      ),
  ];

  /// Schickt die Liste an Media3, ohne den laufenden Titel zu unterbrechen.
  Future<void> _updateNative(QueueSnapshot s) =>
      _native.updateQueue(_items(s), s.nativeIndex);

  /// Führt eine Änderung an der Warteschlange aus (in der Kette) und gleicht
  /// Media3 ab, falls sich die Reihenfolge geändert hat.
  Future<void> _apply(Future<QueueSnapshot> Function() op) => _serial(() async {
    final snap = await op();
    snapshot = snap;
    notifyListeners();
    if (snap.needsNativeUpdate) await _updateNative(snap);
  });

  /// Startet ein Album / eine Playlist ab Titel [start].
  Future<void> playContext(List<Track> tracks, int start, {String? name}) {
    if (tracks.isEmpty) return Future.value();
    return _serial(() async {
      final snap = await queuePlayContext(
        tracks: tracks,
        start: start,
        name: name,
      );
      snapshot = snap;
      notifyListeners();
      // Hier kompletter Austausch: Es soll ja ein neuer Titel starten.
      await _native.setQueue(_items(snap), snap.nativeIndex);
    });
  }

  /// Wie [playContext], aber gemischt (Knopf "Zufällig").
  Future<void> playShuffled(List<Track> tracks, {String? name}) async {
    if (tracks.isEmpty) return;
    if (!shuffle) await toggleShuffle();
    // Start bei 0 genügt: Bei eingeschaltetem Shuffle mischt Rust den Rest.
    // Der erste Titel wäre sonst immer derselbe, deshalb zufällig wählen.
    final start = DateTime.now().microsecondsSinceEpoch % tracks.length;
    await playContext(tracks, start, name: name);
  }

  /// "Zur Warteschlange": spielt vor dem Rest des Albums/der Playlist.
  Future<void> addToQueue(List<Track> tracks) {
    if (tracks.isEmpty) return Future.value();
    return _serial(() async {
      final wasEmpty = !hasTrack;
      final snap = await queueAdd(tracks: tracks);
      snapshot = snap;
      notifyListeners();
      if (wasEmpty) {
        // Es lief noch nichts -> die Titel sofort abspielen.
        await _native.setQueue(_items(snap), snap.nativeIndex);
      } else {
        await _updateNative(snap);
      }
    });
  }

  /// Drag & Drop in der Warteschlange.
  Future<void> moveUpcoming(int from, int to) =>
      _apply(() => queueMove(from: from, to: to));

  Future<void> removeUpcoming(int index) =>
      _apply(() => queueRemove(index: index));

  /// Fügt einen entfernten Titel wieder ein ("Rückgängig").
  Future<void> insertUpcoming(
    int index,
    Track track, {
    required bool intoUser,
  }) =>
      _apply(() => queueInsert(index: index, track: track, intoUser: intoUser));

  Future<void> clearUserQueue() => _apply(queueClearUser);

  /// "Quelle beenden": verwirft den Rest des Albums/der Playlist.
  Future<void> clearContext() => _apply(queueClearContext);

  /// Shuffle ein/aus. Mischt nur das Album/die Playlist, nicht die eigene
  /// Warteschlange.
  Future<void> toggleShuffle() => _apply(() => queueSetShuffle(on_: !shuffle));

  /// Schaltet reihum: aus -> alle wiederholen -> Titel wiederholen -> aus.
  Future<void> cycleRepeat() async {
    final next = switch (repeat) {
      RepeatMode.off => RepeatMode.all,
      RepeatMode.all => RepeatMode.one,
      RepeatMode.one => RepeatMode.off,
    };
    await _apply(() => queueSetRepeat(mode: next));
    // Das eigentliche Wiederholen übernimmt ExoPlayer (bleibt lückenlos).
    await _native.setRepeatMode(next.name);
  }

  /// Sleep-Timer: [minutes] > 0 = Dauer, -1 = bis Titelende, 0 = aus.
  Future<void> setSleepTimer(int minutes) async {
    await _native.setSleepTimer(minutes);
    sleepUntil = minutes > 0
        ? DateTime.now().add(Duration(minutes: minutes))
        : null;
    sleepAtTrackEnd = minutes == -1;
    notifyListeners();
  }

  bool get sleepTimerActive =>
      sleepAtTrackEnd ||
      (sleepUntil != null && DateTime.now().isBefore(sleepUntil!));

  /// Springt zu einem kommenden Titel (Tippen in der Warteschlange).
  /// [upcomingIndex] zählt ab dem ersten Titel nach dem aktuellen.
  Future<void> skipToUpcoming(int upcomingIndex) => _serial(() async {
    final snap = snapshot;
    if (snap == null) return;
    // In der Kette ist der Schnappschuss aktuell, der Index also gültig.
    // Media3 springt; das Titelwechsel-Event zieht Rust automatisch nach.
    await _native.skipTo(snap.nativeIndex + 1 + upcomingIndex);
    await _native.play();
  });

  /// Nach einer Änderung der Klang-Einstellungen aufrufen.
  Future<void> notifyDspChanged() => _native.dspChanged();

  Future<void> togglePlay() =>
      state.isPlaying ? _native.pause() : _native.play();
  Future<void> next() => _native.next();
  Future<void> previous() => _native.previous();
  Future<void> seekTo(Duration pos) => _native.seekTo(pos);
}
