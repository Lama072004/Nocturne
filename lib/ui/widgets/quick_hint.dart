import 'dart:async';

import 'package:flutter/material.dart';

/// Kurze, kleine Bestätigung als "Pille" (z. B. "Zur Warteschlange
/// hinzugefügt").
///
/// WARUM nicht die normale Snackbar? Die Snackbar ist ein breiter Balken,
/// der den Mini-Player verdeckt, und sie bleibt mehrere Sekunden stehen.
/// Beim schnellen Einreihen vieler Titel per Wischgeste war das störend:
/// Der Balken lag dauernd über der Steuerung. Die Pille ist klein, liegt
/// über dem Mini-Player, fängt keine Berührungen ab (man kann einfach
/// weiterwischen) und ist nach höchstens einer Sekunde wieder weg.
///
/// Folgt eine neue Meldung, ersetzt sie die alte sofort, statt sich
/// hinten anzustellen.
void showQuickHint(BuildContext context, String text, {IconData? icon}) {
  final overlay = Overlay.maybeOf(context, rootOverlay: true);
  if (overlay == null) return;
  _current?.remove();
  late final OverlayEntry entry;
  entry = OverlayEntry(
    builder: (_) => _QuickHint(
      text: text,
      icon: icon,
      onDone: () {
        if (_current == entry) _current = null;
        entry.remove();
      },
    ),
  );
  _current = entry;
  overlay.insert(entry);
}

/// Die gerade sichtbare Pille (höchstens eine).
OverlayEntry? _current;

/// Gesamtdauer inklusive Ein- und Ausblenden – bewusst genau 1 s.
const _total = Duration(milliseconds: 1000);
const _fadeIn = 0.12; // Anteil an [_total]
const _fadeOut = 0.22;

class _QuickHint extends StatefulWidget {
  const _QuickHint({required this.text, required this.onDone, this.icon});

  final String text;
  final IconData? icon;
  final VoidCallback onDone;

  @override
  State<_QuickHint> createState() => _QuickHintState();
}

class _QuickHintState extends State<_QuickHint>
    with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: _total,
  );
  bool _removed = false;

  @override
  void initState() {
    super.initState();
    _c.forward().whenCompleteOrCancel(_finish);
  }

  void _finish() {
    if (_removed) return;
    _removed = true;
    // Nicht mitten im Bauen aus dem Overlay nehmen.
    scheduleMicrotask(widget.onDone);
  }

  @override
  void dispose() {
    _removed = true;
    _c.dispose();
    super.dispose();
  }

  /// Sichtbarkeit 0..1: kurz einblenden, stehen, ausblenden.
  double _opacity(double t) {
    if (t < _fadeIn) return t / _fadeIn;
    if (t > 1 - _fadeOut) return (1 - t) / _fadeOut;
    return 1;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final bottom = MediaQuery.paddingOf(context).bottom;
    return Positioned(
      left: 0,
      right: 0,
      // Über Mini-Player und Navigationsleiste, damit nichts verdeckt wird.
      bottom: bottom + 168,
      child: IgnorePointer(
        child: AnimatedBuilder(
          animation: _c,
          builder: (context, child) {
            final o = _opacity(_c.value);
            return Opacity(
              opacity: o,
              // Leichtes Hochgleiten beim Erscheinen.
              child: Transform.translate(
                offset: Offset(0, 8 * (1 - o)),
                child: child,
              ),
            );
          },
          child: Center(
            child: Material(
              color: scheme.inverseSurface,
              elevation: 4,
              shape: const StadiumBorder(),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: 16,
                  vertical: 10,
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    if (widget.icon != null) ...[
                      Icon(
                        widget.icon,
                        size: 18,
                        color: scheme.onInverseSurface,
                      ),
                      const SizedBox(width: 8),
                    ],
                    Flexible(
                      child: Text(
                        widget.text,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.labelLarge?.copyWith(
                          color: scheme.onInverseSurface,
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      ),
    );
  }
}
