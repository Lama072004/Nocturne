import 'package:flutter_test/flutter_test.dart';
import 'package:musicplayer/player/native_player.dart';

/// Prüft das Hochrechnen der Wiedergabeposition.
///
/// Das ist die Stelle, an der wir Akku sparen (keine Positions-Events vom
/// nativen Player). Ein Fehler hier würde einen falschen Fortschrittsbalken
/// bedeuten, deshalb ist es abgesichert.
void main() {
  int now() => DateTime.now().millisecondsSinceEpoch;

  test('pausiert: Position bleibt stehen', () {
    final s = NativePlayerState(
      isPlaying: false,
      positionMs: 10000,
      durationMs: 60000,
      timestampMs: now() - 5000,
    );
    expect(s.currentPosition.inMilliseconds, 10000);
  });

  test('spielt: Position läuft seit dem Zeitstempel weiter', () {
    final s = NativePlayerState(
      isPlaying: true,
      positionMs: 10000,
      durationMs: 60000,
      timestampMs: now() - 5000,
    );
    // Etwas Spielraum, weil zwischen den beiden Uhr-Abfragen Zeit vergeht.
    expect(s.currentPosition.inMilliseconds, inInclusiveRange(15000, 15500));
  });

  test('puffert: Position läuft NICHT weiter', () {
    final s = NativePlayerState(
      isPlaying: true,
      isBuffering: true,
      positionMs: 10000,
      durationMs: 60000,
      timestampMs: now() - 5000,
    );
    expect(s.currentPosition.inMilliseconds, 10000);
  });

  test('geht nie über die Titellänge hinaus', () {
    final s = NativePlayerState(
      isPlaying: true,
      positionMs: 59000,
      durationMs: 60000,
      timestampMs: now() - 5000,
    );
    expect(s.currentPosition.inMilliseconds, 60000);
  });

  test('liest die Daten vom Plattform-Kanal', () {
    final s = NativePlayerState.fromMap({
      'isPlaying': true,
      'index': 2,
      'entryId': 'e7',
      'positionMs': 1234,
      'durationMs': null,
      'timestampMs': 99,
    });
    expect(s.entryId, 'e7');
    expect(s.index, 2);
    expect(s.durationMs, isNull);
  });
}
