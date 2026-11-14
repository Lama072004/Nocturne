import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/player_l10n.dart';
import '../../player/player_controller.dart';
import '../../src/rust/api/models.dart';
import '../../src/rust/api/settings.dart';
import '../../theme/app_theme.dart';
import '../actions.dart';
import '../screens/player_screen.dart';
import 'cover.dart';
import 'progress.dart';

/// Führt eine Player-Geste aus (Mini-Player und Cover im Vollbild-Player
/// teilen sich diese Funktion, damit sie sich gleich verhalten).
void runPlayerSwipe(BuildContext context, PlayerSwipeAction action) {
  final PlayerController player = AppState.instance.player;
  switch (action) {
    case PlayerSwipeAction.none:
      break;
    case PlayerSwipeAction.nextTrack:
      player.next();
    case PlayerSwipeAction.previousTrack:
      player.previous();
    case PlayerSwipeAction.togglePlay:
      player.togglePlay();
    case PlayerSwipeAction.openPlayer:
      openPlayerScreen(context);
  }
}

/// Öffnet den Vollbild-Player von unten (wie bei Spotify).
void openPlayerScreen(BuildContext context) {
  // rootNavigator: Der Player liegt über der Tab-Leiste, nicht innerhalb
  // eines Tabs.
  Navigator.of(context, rootNavigator: true).push(
    PageRouteBuilder(
      pageBuilder: (_, __, ___) => const PlayerScreen(),
      transitionsBuilder: (_, anim, __, child) => SlideTransition(
        position: Tween(
          begin: const Offset(0, 1),
          end: Offset.zero,
        ).animate(CurvedAnimation(parent: anim, curve: Curves.easeOutCubic)),
        child: child,
      ),
    ),
  );
}

/// Prüft, ob ein waagrechtes Wischen ein Titelwechsel sein soll.
///
/// Warum: Die Zurück-Geste von Android beginnt am linken/rechten
/// Bildschirmrand. Ohne diese Prüfung würde sie nebenbei den Titel wechseln.
/// Deshalb zählt eine Geste nur, wenn sie
///  (a) NICHT in der System-Gestenzone begann (mind. 24 px Rand) und
///  (b) weit genug ging (1/4 der Breite) oder deutlich schnell war.
/// Wird auch vom Cover im Vollbild-Player benutzt.
bool isIntentionalHorizontalSwipe(
  BuildContext context, {
  required double startX,
  required double distance,
  required double velocity,
  required double width,
}) {
  final insets = MediaQuery.systemGestureInsetsOf(context);
  final screenW = MediaQuery.sizeOf(context).width;
  final minLeft = insets.left < 24 ? 24.0 : insets.left;
  final minRight = insets.right < 24 ? 24.0 : insets.right;
  if (startX < minLeft || startX > screenW - minRight) return false;
  return distance.abs() >= width / 4 || velocity.abs() >= 700;
}

/// Die schmale Leiste über der Navigation mit dem laufenden Titel.
class MiniPlayer extends StatefulWidget {
  const MiniPlayer({super.key});

  @override
  State<MiniPlayer> createState() => _MiniPlayerState();
}

class _MiniPlayerState extends State<MiniPlayer> {
  final _player = AppState.instance.player;

  /// Cover-Farbe für den Hintergrund; wird nur bei neuem Cover nachgeladen.
  Color? _coverColor;
  String? _colorFor;

  // Zustand der laufenden Wischgeste.
  double _startX = 0;
  double _dx = 0;

  @override
  void initState() {
    super.initState();
    _player.addListener(_updateColor);
    _updateColor();
  }

  @override
  void dispose() {
    _player.removeListener(_updateColor);
    super.dispose();
  }

  void _updateColor() {
    final coverId = _player.currentTrack?.coverArtId;
    if (coverId == _colorFor) return;
    _colorFor = coverId;
    CoverColors.of(coverId).then((c) {
      if (mounted && _colorFor == coverId) setState(() => _coverColor = c);
    });
  }

  void _onHorizontalEnd(DragEndDetails d, PlayerSwipes swipes) {
    final width = context.size?.width ?? 360;
    final v = d.primaryVelocity ?? 0;
    final ok = isIntentionalHorizontalSwipe(
      context,
      startX: _startX,
      distance: _dx,
      velocity: v,
      width: width,
    );
    if (!ok) return;
    // Richtung nach der tatsächlichen Bewegung (links = negativ).
    runPlayerSwipe(context, _dx < 0 ? swipes.left : swipes.right);
  }

  Future<void> _toggleFavorite(Track track) async {
    try {
      await AppState.instance.favorites.toggle(
        StarKind.track,
        track.id,
        track.starred,
      );
    } catch (e) {
      if (mounted) showToast(context, '$e');
    }
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return ListenableBuilder(
      listenable: _player,
      builder: (context, _) {
        final track = _player.currentTrack;
        // Läuft nichts, nimmt der Mini-Player keinen Platz ein.
        if (track == null) return const SizedBox.shrink();
        final theme = Theme.of(context);
        final scheme = theme.colorScheme;
        final swipes = AppState.instance.settings.value.swipes.miniPlayer;
        // Nur leicht einfärben (35 %), damit Text in hellem und dunklem
        // Design lesbar bleibt.
        final bg = _coverColor == null
            ? scheme.surfaceContainer
            : Color.alphaBlend(
                _coverColor!.withValues(alpha: 0.35),
                scheme.surfaceContainer,
              );

        return GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTap: () => openPlayerScreen(context),
          onHorizontalDragStart: (d) {
            _startX = d.globalPosition.dx;
            _dx = 0;
          },
          onHorizontalDragUpdate: (d) => _dx += d.delta.dx,
          onHorizontalDragEnd: (d) => _onHorizontalEnd(d, swipes),
          onVerticalDragEnd: (d) {
            if ((d.primaryVelocity ?? 0) <= -250) {
              runPlayerSwipe(context, swipes.up);
            }
          },
          child: AnimatedContainer(
            duration: const Duration(milliseconds: 400),
            margin: const EdgeInsets.fromLTRB(8, 0, 8, 4),
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(8),
            ),
            clipBehavior: Clip.antiAlias,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(8, 6, 4, 4),
                  child: Row(
                    children: [
                      Cover(coverId: track.coverArtId, size: 40),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              track.title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                fontWeight: FontWeight.w600,
                                color: scheme.onSurface,
                              ),
                            ),
                            if (track.artist != null)
                              Text(
                                track.artist!,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: theme.textTheme.bodySmall?.copyWith(
                                  color: scheme.onSurfaceVariant,
                                ),
                              ),
                          ],
                        ),
                      ),
                      // Herz hört auf den FavoriteStore und springt sofort um.
                      ListenableBuilder(
                        listenable: AppState.instance.favorites,
                        builder: (context, _) {
                          final starred = AppState.instance.favorites
                              .isTrackStarred(track);
                          return IconButton(
                            tooltip: starred
                                ? l.removeFavorite
                                : l.addFavorite,
                            onPressed: () => _toggleFavorite(track),
                            icon: Icon(
                              starred
                                  ? Icons.favorite
                                  : Icons.favorite_border,
                              color: starred ? scheme.primary : null,
                            ),
                          );
                        },
                      ),
                      SizedBox(
                        width: 48,
                        height: 48,
                        child: IconButton(
                          tooltip: _player.state.isPlaying ? l.pause : l.play,
                          onPressed: _player.togglePlay,
                          iconSize: 30,
                          icon: Icon(
                            _player.state.isPlaying
                                ? Icons.pause
                                : Icons.play_arrow,
                          ),
                        ),
                      ),
                    ],
                  ),
                ),
                // Dünner Fortschrittsbalken. Aktualisiert sich nur einmal pro
                // Sekunde: Bei 2 px Höhe sieht man feinere Schritte ohnehin nicht.
                PositionBuilder(
                  interval: const Duration(seconds: 1),
                  builder: (context, s) {
                    final dur = s.durationMs;
                    final value = (dur == null || dur == 0)
                        ? 0.0
                        : (s.currentPosition.inMilliseconds / dur).clamp(
                            0.0,
                            1.0,
                          );
                    return LinearProgressIndicator(
                      value: value,
                      minHeight: 2,
                      backgroundColor: Colors.transparent,
                    );
                  },
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
