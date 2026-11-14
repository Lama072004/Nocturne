import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/player_l10n.dart';
import '../../src/rust/api/models.dart';
import '../../theme/app_theme.dart';
import '../actions.dart';
import '../widgets/context_menu.dart';
import '../widgets/cover.dart';
import '../widgets/mini_player.dart';
import '../widgets/progress.dart';
import '../widgets/track_tile.dart';
import 'equalizer_screen.dart';
import 'lyrics_screen.dart';
import 'queue_screen.dart';

/// Der Vollbild-Player.
///
/// Aufbau von oben nach unten: Kopfzeile, Cover, Titel, Fortschritt,
/// Steuerung, untere Leiste. Das Wichtige (Steuerung, Fortschritt) liegt in
/// der unteren Hälfte, damit man es einhändig erreicht. Bei wenig Platz
/// schrumpft nur das Cover, nie die Bedienelemente.
class PlayerScreen extends StatefulWidget {
  const PlayerScreen({super.key});

  @override
  State<PlayerScreen> createState() => _PlayerScreenState();
}

class _PlayerScreenState extends State<PlayerScreen> {
  final _player = AppState.instance.player;

  /// Hintergrundfarbe passend zum Cover (noch unbearbeitet).
  Color? _color;
  String? _colorFor;

  /// Während der Nutzer am Regler zieht, zeigen wir SEINE Position an und
  /// nicht die echte, sonst springt der Regler unter dem Finger zurück.
  double? _dragValue;

  // Zustand der laufenden Wischgeste auf dem Cover.
  double _swipeStartX = 0;
  double _swipeDx = 0;

  @override
  void initState() {
    super.initState();
    _player.addListener(_onPlayer);
    _onPlayer();
  }

  @override
  void dispose() {
    _player.removeListener(_onPlayer);
    super.dispose();
  }

  void _onPlayer() {
    final coverId = _player.currentTrack?.coverArtId;
    // Farbe nur neu bestimmen, wenn sich das Cover wirklich geändert hat.
    if (coverId != _colorFor) {
      _colorFor = coverId;
      CoverColors.of(coverId).then((c) {
        if (mounted && _colorFor == coverId) setState(() => _color = c);
      });
    }
    if (mounted) setState(() {});
  }

  /// Macht die Cover-Farbe als Hintergrund tauglich: im dunklen Design
  /// abdunkeln, im hellen aufhellen, damit Text und Symbole (onSurface)
  /// immer genug Kontrast haben, egal wie grell das Cover ist.
  Color _tint(Color c, Brightness b) {
    final hsl = HSLColor.fromColor(c);
    final l = b == Brightness.dark
        ? math.min(hsl.lightness, 0.30)
        : math.max(hsl.lightness, 0.80);
    final s = math.min(hsl.saturation, 0.75);
    return hsl.withLightness(l).withSaturation(s).toColor();
  }

  // ---- Blätter ----------------------------------------------------------

  /// Drei-Punkte-Blatt: Sleep-Timer, Klang, Aktionen zum Titel.
  void _showMoreSheet(Track track) {
    final l = L.of(context);
    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheet) => SafeArea(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            ListTile(
              leading: const Icon(Icons.bedtime_outlined),
              title: Text(l.sleepTimer),
              onTap: () {
                Navigator.of(sheet).pop();
                _showSleepTimerSheet();
              },
            ),
            ListTile(
              leading: const Icon(Icons.equalizer),
              title: Text(l.soundMenu),
              onTap: () {
                Navigator.of(sheet).pop();
                Navigator.of(context).push(
                  MaterialPageRoute(builder: (_) => const EqualizerScreen()),
                );
              },
            ),
            ListTile(
              leading: const Icon(Icons.more_horiz),
              title: Text(l.trackActions),
              onTap: () {
                Navigator.of(sheet).pop();
                showTrackMenu(context, track);
              },
            ),
          ],
        ),
      ),
    );
  }

  /// Auswahl für den Sleep-Timer; zeigt oben, wann ein laufender endet.
  void _showSleepTimerSheet() {
    final l = L.of(context);
    final outer = context;
    final theme = Theme.of(context);

    String? status() {
      if (_player.sleepAtTrackEnd) return l.sleepUntilTrackEnd;
      final until = _player.sleepUntil;
      if (until != null && _player.sleepTimerActive) {
        final t = MaterialLocalizations.of(
          outer,
        ).formatTimeOfDay(TimeOfDay.fromDateTime(until));
        return l.sleepEndsAt(t);
      }
      return null;
    }

    showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (sheet) {
        void choose(int minutes) {
          _player.setSleepTimer(minutes);
          Navigator.of(sheet).pop();
          if (minutes != 0 && outer.mounted) showToast(outer, l.sleepSet);
        }

        final st = status();
        return SafeArea(
          child: ListView(
            shrinkWrap: true,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                child: Text(
                  l.sleepTimer,
                  style: theme.textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.bold,
                  ),
                ),
              ),
              if (st != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 8),
                  child: Text(
                    st,
                    style: theme.textTheme.bodyMedium?.copyWith(
                      color: theme.colorScheme.primary,
                    ),
                  ),
                ),
              for (final m in const [5, 10, 15, 30, 45, 60])
                ListTile(title: Text(l.minutes(m)), onTap: () => choose(m)),
              ListTile(title: Text(l.endOfTrack), onTap: () => choose(-1)),
              // "Aus" nur anbieten, wenn überhaupt ein Timer läuft.
              if (st != null)
                ListTile(
                  leading: const Icon(Icons.timer_off_outlined),
                  title: Text(l.sleepOff),
                  onTap: () => choose(0),
                ),
            ],
          ),
        );
      },
    );
  }

  // ---- Gesten -----------------------------------------------------------

  void _onCoverSwipeEnd(DragEndDetails d, double width) {
    final swipes = AppState.instance.settings.value.swipes.miniPlayer;
    final ok = isIntentionalHorizontalSwipe(
      context,
      startX: _swipeStartX,
      distance: _swipeDx,
      velocity: d.primaryVelocity ?? 0,
      width: width,
    );
    if (!ok) return;
    runPlayerSwipe(context, _swipeDx < 0 ? swipes.left : swipes.right);
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

  // ---- Bau --------------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final track = _player.currentTrack;
    final snap = _player.snapshot;
    final bg = theme.scaffoldBackgroundColor;
    final top = _color == null ? bg : _tint(_color!, theme.brightness);

    return Scaffold(
      body: AnimatedContainer(
        // Weicher Übergang, wenn der nächste Titel eine andere Farbe hat.
        duration: const Duration(milliseconds: 500),
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [top, bg],
            stops: const [0, 0.8],
          ),
        ),
        child: SafeArea(
          child: track == null
              ? const SizedBox.shrink()
              : Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 24),
                  // AUFTEILUNG (Einhand-Bedienung): Cover direkt unter die
                  // Kopfzeile, Titel und Knöpfe direkt unter das Cover, und
                  // ALLER übrige Platz kommt nach unten.
                  //
                  // Vorher stand das Cover mittig in einer dehnbaren Fläche:
                  // Der freie Platz lag über UND unter dem Cover, und die
                  // Knöpfe klebten am unteren Bildschirmrand – mit dem
                  // Daumen schlecht zu erreichen, weil man das Handy dafür
                  // umgreifen muss. Jetzt liegen sie spürbar höher.
                  //
                  // Wie hoch genau? Der Nutzer wollte die Mitte zwischen der
                  // alten Lage (Play-Knopf bei 88 % der Höhe) und "ganz
                  // nach oben" (69 %), also etwa 78 %. Das ergibt sich, wenn
                  // der freie Platz zur HÄLFTE über dem Cover und zur Hälfte
                  // unter den Knöpfen liegt. Die Rechnung macht
                  // [_PlayerBodyLayout] (siehe dort, warum nicht mit
                  // Column/Spacer).
                  child: Column(
                    children: [
                      _buildHeader(l, theme, snap?.contextName, track),
                      Expanded(
                        child: CustomMultiChildLayout(
                          delegate: _PlayerBodyLayout(),
                          children: [
                            LayoutId(
                              id: _PlayerSlot.cover,
                              child: _buildCover(track),
                            ),
                            LayoutId(
                              id: _PlayerSlot.controls,
                              child: Column(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  _buildTitleRow(theme, track),
                                  _buildProgress(theme),
                                  _buildControls(l, theme),
                                  _buildBottomRow(l, scheme),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
        ),
      ),
    );
  }

  /// Pfeil nach unten · "Wiedergabe von" + Quelle · Drei-Punkte-Knopf.
  Widget _buildHeader(L l, ThemeData theme, String? source, Track track) {
    return Row(
      children: [
        IconButton(
          tooltip: l.closePlayer,
          onPressed: () => Navigator.of(context).pop(),
          icon: const Icon(Icons.keyboard_arrow_down, size: 32),
        ),
        Expanded(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              // Ohne bekannte Quelle (z. B. Sprachsuche) lieber nichts als
              // ein verwaistes "Wiedergabe von" ohne Namen.
              if (source != null && source.isNotEmpty) ...[
                Text(
                  l.playingFrom,
                  maxLines: 1,
                  style: theme.textTheme.labelSmall?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
                Text(
                  source,
                  textAlign: TextAlign.center,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.labelLarge,
                ),
              ],
            ],
          ),
        ),
        IconButton(
          tooltip: l.moreOptions,
          onPressed: () => _showMoreSheet(track),
          icon: const Icon(Icons.more_vert),
        ),
      ],
    );
  }

  /// Quadratisches, abgerundetes Cover mit Wisch-Gesten.
  Widget _buildCover(Track track) {
    return LayoutBuilder(
      builder: (context, c) {
        final size = math.max(0.0, c.biggest.shortestSide);
        // Genau so groß wie das Cover (nicht wie die ganze Fläche), damit
        // die Knöpfe direkt darunter anschließen.
        return SizedBox.square(
          dimension: size,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onHorizontalDragStart: (d) {
              _swipeStartX = d.globalPosition.dx;
              _swipeDx = 0;
            },
            onHorizontalDragUpdate: (d) => _swipeDx += d.delta.dx,
            onHorizontalDragEnd: (d) => _onCoverSwipeEnd(d, size),
            // Nach unten wischen schließt den Player.
            onVerticalDragEnd: (d) {
              if ((d.primaryVelocity ?? 0) >= 250) {
                Navigator.of(context).pop();
              }
            },
            child: Cover(coverId: track.coverArtId, size: size, radius: 12),
          ),
        );
      },
    );
  }

  /// Titel (fett) + Künstler, rechts das Herz.
  Widget _buildTitleRow(ThemeData theme, Track track) {
    final l = L.of(context);
    return Row(
      children: [
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                track.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: theme.textTheme.titleLarge?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
              ),
              if (track.artist != null)
                Text(
                  track.artist!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodyLarge?.copyWith(
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
            ],
          ),
        ),
        // Hört auf den FavoriteStore, damit das Herz sofort umspringt.
        ListenableBuilder(
          listenable: AppState.instance.favorites,
          builder: (context, _) {
            final starred = AppState.instance.favorites.isTrackStarred(track);
            return IconButton(
              tooltip: starred ? l.removeFavorite : l.addFavorite,
              iconSize: 28,
              onPressed: () => _toggleFavorite(track),
              icon: Icon(
                starred ? Icons.favorite : Icons.favorite_border,
                color: starred ? theme.colorScheme.primary : null,
              ),
            );
          },
        ),
      ],
    );
  }

  /// Regler mit Zeiten. Beim Ziehen zeigt er die gezogene Position.
  Widget _buildProgress(ThemeData theme) {
    return PositionBuilder(
      builder: (context, s) {
        final dur = (s.durationMs ?? 0).toDouble();
        final pos = s.currentPosition.inMilliseconds.toDouble();
        final value = (_dragValue ?? pos).clamp(0.0, dur > 0 ? dur : 1.0);
        return Column(
          children: [
            SliderTheme(
              data: SliderTheme.of(context).copyWith(
                trackHeight: 3,
                thumbShape: const RoundSliderThumbShape(enabledThumbRadius: 6),
                overlayShape: const RoundSliderOverlayShape(overlayRadius: 16),
              ),
              child: Slider(
                max: dur > 0 ? dur : 1.0,
                value: value,
                onChanged: dur > 0
                    ? (v) => setState(() => _dragValue = v)
                    : null,
                // Erst beim Loslassen springen: Ein Seek pro Geste statt
                // dutzender Netzwerk-Anfragen.
                onChangeEnd: (v) {
                  _player.seekTo(Duration(milliseconds: v.round()));
                  setState(() => _dragValue = null);
                },
              ),
            ),
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                mainAxisAlignment: MainAxisAlignment.spaceBetween,
                children: [
                  Text(
                    formatDuration(value ~/ 1000),
                    style: theme.textTheme.bodySmall,
                  ),
                  Text(
                    formatDuration(dur ~/ 1000),
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ],
        );
      },
    );
  }

  /// Shuffle · Zurück · Play/Pause · Weiter · Wiederholen.
  Widget _buildControls(L l, ThemeData theme) {
    final accent = theme.colorScheme.primary;
    final playing = _player.state.isPlaying;
    return Row(
      mainAxisAlignment: MainAxisAlignment.spaceEvenly,
      children: [
        IconButton(
          tooltip: l.shuffleTip,
          onPressed: _player.toggleShuffle,
          icon: const Icon(Icons.shuffle),
          // Aktiv = Akzentfarbe.
          color: _player.shuffle ? accent : null,
        ),
        IconButton(
          tooltip: l.previousTrack,
          onPressed: _player.previous,
          icon: const Icon(Icons.skip_previous),
          iconSize: 40,
        ),
        // Großer runder Play-Knopf (mind. 64 x 64).
        Material(
          color: theme.colorScheme.onSurface,
          shape: const CircleBorder(),
          child: IconButton(
            tooltip: playing ? l.pause : l.play,
            onPressed: _player.togglePlay,
            color: theme.colorScheme.surface,
            iconSize: 40,
            constraints: const BoxConstraints.tightFor(width: 68, height: 68),
            icon: Icon(playing ? Icons.pause : Icons.play_arrow),
          ),
        ),
        IconButton(
          tooltip: l.nextTrack,
          onPressed: _player.next,
          icon: const Icon(Icons.skip_next),
          iconSize: 40,
        ),
        IconButton(
          tooltip: l.repeatTip,
          onPressed: _player.cycleRepeat,
          // Bei "Titel" das Symbol mit der 1.
          icon: Icon(
            _player.repeat == RepeatMode.one ? Icons.repeat_one : Icons.repeat,
          ),
          color: _player.repeat == RepeatMode.off ? null : accent,
        ),
      ],
    );
  }

  /// Links Sleep-Timer, rechts Songtext und Warteschlange.
  Widget _buildBottomRow(L l, ColorScheme scheme) {
    return Row(
      children: [
        IconButton(
          tooltip: l.sleepTimer,
          onPressed: _showSleepTimerSheet,
          icon: const Icon(Icons.bedtime_outlined),
          color: _player.sleepTimerActive ? scheme.primary : null,
        ),
        const Spacer(),
        IconButton(
          tooltip: l.lyrics,
          onPressed: () => Navigator.of(
            context,
          ).push(MaterialPageRoute(builder: (_) => const LyricsScreen())),
          icon: const Icon(Icons.lyrics_outlined),
        ),
        IconButton(
          tooltip: l.queue,
          onPressed: () => Navigator.of(
            context,
          ).push(MaterialPageRoute(builder: (_) => const QueueScreen())),
          icon: const Icon(Icons.queue_music),
        ),
      ],
    );
  }
}

/// Die zwei Blöcke unter der Kopfzeile des Players.
enum _PlayerSlot { cover, controls }

/// Ordnet Cover und Steuerung im Player an.
///
/// Regeln:
///  1. Die Steuerung (Titel, Fortschritt, Knöpfe, untere Reihe) bekommt
///     immer ihre volle Höhe – sie wird nie gequetscht.
///  2. Das Cover ist quadratisch und so groß wie möglich, höchstens so breit
///     wie der Bildschirm. Ist es zu wenig Platz, schrumpft nur das Cover.
///  3. Der übrige Platz wird HALBIERT: eine Hälfte über das Cover, die
///     andere unter die Steuerung. Damit liegen die Knöpfe genau in der
///     Mitte zwischen "ganz unten am Rand" und "direkt unter der
///     Kopfzeile" – gut mit dem Daumen erreichbar.
///
/// WARUM nicht einfach Column mit Spacer/Flexible? Mit Flex-Anteilen hängt
/// die Aufteilung davon ab, wie viel Platz das Cover "nicht braucht", und
/// lässt sich nur ungefähr steuern. Hier wird die Steuerung zuerst gemessen
/// und dann exakt gerechnet – das Ergebnis ist auf jedem Bildschirm
/// dasselbe Verhältnis.
class _PlayerBodyLayout extends MultiChildLayoutDelegate {
  /// Abstand zwischen Cover und Titel.
  static const _gap = 20.0;

  /// Mindestabstand oben (unter der Kopfzeile) und unten (zum Rand).
  static const _minTop = 12.0;
  static const _minBottom = 16.0;

  @override
  void performLayout(Size size) {
    final controls = layoutChild(
      _PlayerSlot.controls,
      BoxConstraints(minWidth: size.width, maxWidth: size.width),
    );
    final room = size.height - controls.height - _gap - _minTop - _minBottom;
    final cover = math.max(0.0, math.min(size.width, room));
    layoutChild(_PlayerSlot.cover, BoxConstraints.tight(Size.square(cover)));

    // Übrigen Platz halbieren (Regel 3).
    final free = math.max(0.0, room - cover);
    final coverTop = _minTop + free / 2;
    positionChild(
      _PlayerSlot.cover,
      Offset((size.width - cover) / 2, coverTop),
    );
    positionChild(_PlayerSlot.controls, Offset(0, coverTop + cover + _gap));
  }

  @override
  bool shouldRelayout(_PlayerBodyLayout oldDelegate) => false;
}
