import 'dart:async';

import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../player/native_player.dart';

/// Baut seinen Inhalt regelmäßig neu, solange Musik läuft UND das Widget
/// sichtbar ist.
///
/// WARUM ein eigener Timer statt Positions-Events vom nativen Player?
/// Der native Player schickt nur bei echten Änderungen ein Event (siehe
/// `PlayerBridge.kt`). Die laufende Position rechnet Dart selbst hoch. Der
/// Timer existiert nur, solange dieses Widget im Baum hängt: Ist der
/// Bildschirm aus oder die App im Hintergrund, läuft hier nichts und es wird
/// kein Akku verbraucht.
class PositionBuilder extends StatefulWidget {
  const PositionBuilder({
    super.key,
    required this.builder,
    this.interval = const Duration(milliseconds: 500),
  });

  final Widget Function(BuildContext context, NativePlayerState state) builder;
  final Duration interval;

  @override
  State<PositionBuilder> createState() => _PositionBuilderState();
}

class _PositionBuilderState extends State<PositionBuilder> {
  Timer? _timer;
  final _player = AppState.instance.player;

  @override
  void initState() {
    super.initState();
    _player.addListener(_sync);
    _sync();
  }

  /// Timer nur laufen lassen, wenn sich die Position auch wirklich bewegt.
  void _sync() {
    final moving = _player.state.isPlaying && !_player.state.isBuffering;
    if (moving && _timer == null) {
      _timer = Timer.periodic(widget.interval, (_) => setState(() {}));
    } else if (!moving && _timer != null) {
      _timer!.cancel();
      _timer = null;
    }
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    _player.removeListener(_sync);
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _player.state);
}
