import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/player_l10n.dart';
import '../../src/rust/api/models.dart';
import '../../src/rust/api/queue.dart';
import '../widgets/async_view.dart';
import '../widgets/track_tile.dart';

/// Die Warteschlange nach dem Spotify-Modell:
///
///   Läuft gerade
///   Als Nächstes in der Warteschlange   <- eigene Titel ("Zur Warteschlange")
///   Als Nächstes von: (Album/Playlist)  <- Rest des Kontexts
///
/// Alle kommenden Titel lassen sich frei verschieben, auch zwischen den
/// beiden Bereichen. Die Regeln dafür stehen in Rust (`core/queue.rs`); diese
/// Seite zeigt nur an und meldet "von Position A nach B".
class QueueScreen extends StatelessWidget {
  const QueueScreen({super.key});

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final player = AppState.instance.player;
    final theme = Theme.of(context);

    // Zustandsknopf (Shuffle/Wiederholen) in der Kopfzeile: Er hört selbst
    // auf den Player, weil die AppBar nicht im ListenableBuilder des Bodys liegt.
    Widget toggle({
      required String tooltip,
      required IconData Function() icon,
      required bool Function() active,
      required VoidCallback onPressed,
    }) {
      return ListenableBuilder(
        listenable: player,
        builder: (context, _) => IconButton(
          tooltip: tooltip,
          onPressed: onPressed,
          icon: Icon(icon()),
          color: active() ? theme.colorScheme.primary : null,
        ),
      );
    }

    return Scaffold(
      appBar: AppBar(
        title: Text(l.queue),
        actions: [
          toggle(
            tooltip: l.shuffleTip,
            icon: () => Icons.shuffle,
            active: () => player.shuffle,
            onPressed: player.toggleShuffle,
          ),
          toggle(
            tooltip: l.repeatTip,
            icon: () => player.repeat == RepeatMode.one
                ? Icons.repeat_one
                : Icons.repeat,
            active: () => player.repeat != RepeatMode.off,
            onPressed: player.cycleRepeat,
          ),
        ],
      ),
      body: ListenableBuilder(
        listenable: player,
        builder: (context, _) {
          final snap = player.snapshot;
          final current = snap?.current;
          if (snap == null || current == null) {
            // Sackgasse vermeiden: Knopf zurück zur Bibliothek. Wir schließen
            // nur Warteschlange und Player. Ein Tab-Wechsel per
            // AppShell.goToTab ist von hier nicht sauber möglich, weil diese
            // Seite über dem Root-Navigator liegt und die Shell kein
            // Vorfahr ihres Contexts ist.
            return EmptyHint(
              icon: Icons.queue_music,
              text: l.queueEmpty,
              action: FilledButton(
                onPressed: () => Navigator.of(
                  context,
                  rootNavigator: true,
                ).popUntil((r) => r.isFirst),
                child: Text(l.toLibrary),
              ),
            );
          }

          // Beide Bereiche als EINE Liste, damit man von einem Bereich in
          // den anderen ziehen kann. Die Überschriften hängen jeweils am
          // ersten Eintrag ihres Bereichs.
          final userLen = snap.userQueue.length;
          final List<QueueItem> upcoming = [...snap.userQueue, ...snap.context];
          final swipes = AppState.instance.settings.value.swipes.queueRows;

          return CustomScrollView(
            slivers: [
              SliverToBoxAdapter(child: _Header(l.playingNow)),
              SliverToBoxAdapter(
                child: TrackTile(
                  track: current.track,
                  swipes: swipes,
                  onTap: () {},
                ),
              ),
              // Dezenter Hinweis, solange die eigene Warteschlange leer ist.
              if (userLen == 0)
                SliverToBoxAdapter(
                  child: Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                    child: Text(
                      l.queueHint,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: theme.hintColor,
                      ),
                    ),
                  ),
                ),
              SliverReorderableList(
                itemCount: upcoming.length,
                // Die gezogene Zeile schwebt in einer eigenen Ebene (Overlay)
                // über der Seite. Dort fehlt das "Material" der Seite, das
                // die Zeile für ihren Tipp-Effekt braucht – ohne diese Hülle
                // wirft Flutter beim Ziehen einen Fehler. Der Schatten zeigt
                // zugleich, dass die Zeile gerade "angehoben" ist.
                proxyDecorator: (child, _, _) => Material(
                  elevation: 6,
                  color: theme.colorScheme.surfaceContainerHigh,
                  child: child,
                ),
                onReorder: (from, to) {
                  // Flutter meldet das Ziel VOR dem Herausnehmen; Rust
                  // erwartet es DANACH. Beim Verschieben nach unten liegt
                  // dazwischen genau eine Position.
                  if (to > from) to -= 1;
                  player.moveUpcoming(from, to);
                },
                itemBuilder: (context, i) {
                  final item = upcoming[i];
                  return Column(
                    // Schlüssel = Eintrags-ID: Flutter erkennt die Zeile nach
                    // dem Verschieben wieder, auch wenn derselbe Song
                    // mehrfach in der Liste steht.
                    key: ValueKey(item.entryId),
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (i == 0 && userLen > 0)
                        _Header(
                          l.nextInQueue,
                          action: TextButton(
                            onPressed: player.clearUserQueue,
                            child: Text(l.clearQueue),
                          ),
                        ),
                      if (i == userLen)
                        _Header(
                          l.nextFrom(snap.contextName),
                          action: TextButton(
                            onPressed: player.clearContext,
                            child: Text(l.endSource),
                          ),
                        ),
                      TrackTile(
                        track: item.track,
                        swipes: swipes,
                        queueIndex: i,
                        onTap: () => player.skipToUpcoming(i),
                        // Griff zum Verschieben (48 px). Nur der Griff startet
                        // das Ziehen, damit Wischen und Scrollen nicht stören.
                        trailing: ReorderableDragStartListener(
                          index: i,
                          child: Tooltip(
                            message: l.dragToReorder,
                            child: const SizedBox(
                              width: 48,
                              height: 48,
                              child: Icon(Icons.drag_handle),
                            ),
                          ),
                        ),
                      ),
                    ],
                  );
                },
              ),
              const SliverToBoxAdapter(child: SizedBox(height: 32)),
            ],
          );
        },
      ),
    );
  }
}

/// Abschnittsüberschrift, optional mit Knopf rechts.
class _Header extends StatelessWidget {
  const _Header(this.text, {this.action});

  final String text;
  final Widget? action;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 16, 8, 4),
    child: Row(
      children: [
        Expanded(
          child: Text(
            text,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: Theme.of(
              context,
            ).textTheme.titleSmall?.copyWith(fontWeight: FontWeight.bold),
          ),
        ),
        if (action != null) action!,
      ],
    ),
  );
}
