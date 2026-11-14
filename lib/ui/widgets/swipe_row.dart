import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../../l10n.dart';
import '../../src/rust/api/settings.dart';

/// Eine Zeile mit frei belegbaren Wischgesten.
///
/// Pro Richtung gibt es zwei Stufen:
///   - KURZ: ab [_shortThreshold] Pixeln
///   - LANG: ab [_longFraction] der Zeilenbreite
/// Das ergibt bis zu vier Aktionen pro Zeile (siehe `SwipeSet` in Rust).
///
/// WARUM selbst gebaut statt `Dismissible`?
/// `Dismissible` kennt nur "weggewischt oder nicht" und entfernt die Zeile.
/// Wir brauchen zwei Schwellen je Richtung, und die Zeile soll danach
/// zurückfedern (wie bei Spotify), nicht verschwinden.
class SwipeRow extends StatefulWidget {
  const SwipeRow({
    super.key,
    required this.actions,
    required this.onAction,
    required this.child,
  });

  final SwipeSet actions;
  final void Function(SwipeAction action) onAction;
  final Widget child;

  @override
  State<SwipeRow> createState() => _SwipeRowState();
}

class _SwipeRowState extends State<SwipeRow>
    with SingleTickerProviderStateMixin {
  static const double _shortThreshold = 72;
  static const double _longFraction = 0.55;

  /// Aktuelle Verschiebung der Zeile in Pixeln (+ = nach rechts).
  double _dx = 0;

  /// Welche Aktion beim Loslassen ausgelöst würde. Merken wir uns, um nur
  /// beim WECHSEL der Stufe zu vibrieren.
  SwipeAction _armed = SwipeAction.none;

  /// Animiert das Zurückfedern.
  late final AnimationController _back = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 180),
  );
  double _backFrom = 0;

  @override
  void initState() {
    super.initState();
    _back.addListener(() {
      setState(
        () => _dx = _backFrom * (1 - Curves.easeOut.transform(_back.value)),
      );
    });
  }

  @override
  void dispose() {
    _back.dispose();
    super.dispose();
  }

  /// Bestimmt aus der Verschiebung die Aktion.
  /// Ist für "lang" nichts belegt, gilt auch dort die kurze Aktion. So
  /// passiert nie "nichts", nur weil man etwas zu weit gewischt hat.
  SwipeAction _actionFor(double dx, double width) {
    final a = widget.actions;
    final dist = dx.abs();
    if (dist < _shortThreshold) return SwipeAction.none;
    final isLong = dist >= width * _longFraction;
    final (short, long) = dx > 0
        ? (a.rightShort, a.rightLong)
        : (a.leftShort, a.leftLong);
    return (isLong && long != SwipeAction.none) ? long : short;
  }

  bool _directionEnabled(bool right) {
    final a = widget.actions;
    return right
        ? (a.rightShort != SwipeAction.none || a.rightLong != SwipeAction.none)
        : (a.leftShort != SwipeAction.none || a.leftLong != SwipeAction.none);
  }

  void _onUpdate(DragUpdateDetails d, double width) {
    var next = _dx + d.delta.dx;
    // Richtungen ohne Belegung lassen sich gar nicht erst ziehen.
    if (next > 0 && !_directionEnabled(true)) next = 0;
    if (next < 0 && !_directionEnabled(false)) next = 0;
    final armed = _actionFor(next, width);
    if (armed != _armed) {
      _armed = armed;
      // Kurzes Vibrieren beim Überschreiten einer Schwelle: Der Nutzer
      // spürt, welche Aktion gleich ausgelöst wird, ohne hinzusehen.
      if (armed != SwipeAction.none) HapticFeedback.selectionClick();
    }
    setState(() => _dx = next);
  }

  void _onEnd() {
    final action = _armed;
    _armed = SwipeAction.none;
    _backFrom = _dx;
    _back.forward(from: 0);
    if (action != SwipeAction.none) widget.onAction(action);
  }

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final width = constraints.maxWidth;
        final action = _actionFor(_dx, width);
        return GestureDetector(
          // Nur horizontale Gesten abfangen, damit die Liste weiter normal
          // senkrecht scrollt.
          onHorizontalDragUpdate: (d) => _onUpdate(d, width),
          onHorizontalDragEnd: (_) => _onEnd(),
          onHorizontalDragCancel: _onEnd,
          behavior: HitTestBehavior.opaque,
          child: Stack(
            children: [
              // Hintergrund mit Symbol und Text der Aktion. Wird nur gebaut,
              // solange wirklich gewischt wird (spart Arbeit in langen Listen).
              if (_dx != 0)
                Positioned.fill(
                  child: _SwipeBackground(action: action, toRight: _dx > 0),
                ),
              Transform.translate(
                offset: Offset(_dx, 0),
                child: ColoredBox(
                  // Deckende Farbe, damit der Hintergrund nicht durchscheint.
                  color: Theme.of(context).scaffoldBackgroundColor,
                  child: widget.child,
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

class _SwipeBackground extends StatelessWidget {
  const _SwipeBackground({required this.action, required this.toRight});

  final SwipeAction action;
  final bool toRight;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    final active = action != SwipeAction.none;
    // Rot nur für "Entfernen", sonst die Akzentfarbe.
    final color = !active
        ? scheme.surfaceContainer
        : action == SwipeAction.removeFromQueue
        ? scheme.error
        : scheme.primary;
    final fg = !active
        ? scheme.onSurface
        : action == SwipeAction.removeFromQueue
        ? scheme.onError
        : scheme.onPrimary;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 120),
      color: color,
      padding: const EdgeInsets.symmetric(horizontal: 20),
      alignment: toRight ? Alignment.centerLeft : Alignment.centerRight,
      child: active
          ? Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Icon(swipeActionIcon(action), color: fg),
                const SizedBox(width: 8),
                Text(
                  L.of(context).swipeActionName(action),
                  style: TextStyle(color: fg),
                ),
              ],
            )
          : null,
    );
  }
}

IconData swipeActionIcon(SwipeAction a) => switch (a) {
  SwipeAction.none => Icons.block,
  SwipeAction.addToQueue => Icons.queue_music,
  SwipeAction.toggleFavorite => Icons.favorite_border,
  SwipeAction.download => Icons.download_outlined,
  SwipeAction.addToPlaylist => Icons.playlist_add,
  SwipeAction.goToAlbum => Icons.album_outlined,
  SwipeAction.goToArtist => Icons.person_outline,
  SwipeAction.removeFromQueue => Icons.delete_outline,
};
