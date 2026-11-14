import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/models.dart';
import '../widgets/async_view.dart';
import '../widgets/progress.dart';

/// Songtext zum laufenden Titel. Hat der Server Zeitstempel, läuft der
/// Text mit und die aktuelle Zeile ist hervorgehoben; Tippen auf eine Zeile
/// springt dorthin.
class LyricsScreen extends StatefulWidget {
  const LyricsScreen({super.key});

  @override
  State<LyricsScreen> createState() => _LyricsScreenState();
}

class _LyricsScreenState extends State<LyricsScreen> {
  final _player = AppState.instance.player;
  final _scroll = ScrollController();

  String? _trackId;
  Future<Lyrics?>? _future;

  /// Zuletzt hervorgehobene Zeile. Gescrollt wird nur, wenn sie WECHSELT,
  /// nicht bei jedem Zeit-Tick.
  int _activeLine = -1;

  /// Ungefähre Zeilenhöhe zum Mitscrollen. Eine exakte Messung jeder Zeile
  /// wäre viel Aufwand für einen kaum sichtbaren Unterschied.
  static const _lineHeight = 44.0;

  @override
  void initState() {
    super.initState();
    _player.addListener(_onPlayer);
    _onPlayer();
  }

  @override
  void dispose() {
    _player.removeListener(_onPlayer);
    _scroll.dispose();
    super.dispose();
  }

  /// Wechselt der Titel, wird der passende Text geladen.
  void _onPlayer() {
    final id = _player.currentTrack?.id;
    if (id == _trackId) return;
    setState(() {
      _trackId = id;
      _activeLine = -1;
      _future = id == null ? null : rust.lyrics(songId: id);
    });
  }

  /// Index der Zeile, die zur Position gehört: die letzte, deren Startzeit
  /// schon erreicht ist.
  int _lineAt(Lyrics lyrics, int positionMs) {
    var index = -1;
    for (var i = 0; i < lyrics.lines.length; i++) {
      final start = lyrics.lines[i].startMs;
      if (start != null && start <= positionMs) index = i;
    }
    return index;
  }

  void _scrollTo(int line) {
    if (!_scroll.hasClients || line < 0) return;
    // Aktive Zeile etwa im oberen Drittel halten.
    final target = (line * _lineHeight - 160).clamp(
      0.0,
      _scroll.position.maxScrollExtent,
    );
    _scroll.animateTo(
      target,
      duration: const Duration(milliseconds: 350),
      curve: Curves.easeOut,
    );
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final track = _player.currentTrack;
    final future = _future;

    return Scaffold(
      appBar: AppBar(title: Text(track?.title ?? l.lyrics)),
      body: future == null
          ? EmptyHint(icon: Icons.lyrics_outlined, text: l.noLyrics)
          : AsyncView<Lyrics?>(
              future: future,
              onRetry: () => setState(() {
                _future = rust.lyrics(songId: _trackId!);
              }),
              builder: (context, lyrics) {
                if (lyrics == null || lyrics.lines.isEmpty) {
                  return EmptyHint(
                    icon: Icons.lyrics_outlined,
                    text: l.noLyrics,
                  );
                }
                // PositionBuilder tickt nur, solange diese Seite sichtbar ist
                // und Musik läuft (siehe dort).
                return PositionBuilder(
                  interval: const Duration(milliseconds: 300),
                  builder: (context, state) {
                    final active = lyrics.synced
                        ? _lineAt(lyrics, state.currentPosition.inMilliseconds)
                        : -1;
                    if (active != _activeLine) {
                      _activeLine = active;
                      // Erst nach dem Zeichnen scrollen; während des Bauens
                      // darf man die Liste nicht bewegen.
                      WidgetsBinding.instance.addPostFrameCallback(
                        (_) => _scrollTo(active),
                      );
                    }
                    return ListView.builder(
                      controller: _scroll,
                      padding: const EdgeInsets.fromLTRB(24, 16, 24, 120),
                      itemCount: lyrics.lines.length,
                      itemBuilder: (context, i) {
                        final line = lyrics.lines[i];
                        final isActive = i == active;
                        return InkWell(
                          onTap: line.startMs == null
                              ? null
                              : () => _player.seekTo(
                                  Duration(milliseconds: line.startMs!),
                                ),
                          child: Padding(
                            padding: const EdgeInsets.symmetric(vertical: 8),
                            child: Text(
                              line.text.isEmpty ? '♪' : line.text,
                              style: theme.textTheme.titleLarge?.copyWith(
                                fontWeight: isActive
                                    ? FontWeight.bold
                                    : FontWeight.w500,
                                // Bei nicht synchronisiertem Text sind alle
                                // Zeilen gleich gut lesbar.
                                color: (isActive || !lyrics.synced)
                                    ? theme.colorScheme.onSurface
                                    : theme.hintColor,
                              ),
                            ),
                          ),
                        );
                      },
                    );
                  },
                );
              },
            ),
    );
  }
}
