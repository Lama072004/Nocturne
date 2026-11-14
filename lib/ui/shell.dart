import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../app_state.dart';
import '../l10n.dart';
import 'screens/home_screen.dart';
import 'screens/library_screen.dart';
import 'screens/search_screen.dart';
import 'widgets/mini_player.dart';

/// Das Grundgerüst nach der Anmeldung: drei Tabs unten (Home, Suche,
/// Bibliothek), darüber der Mini-Player – wie bei Spotify.
///
/// WARUM nur drei Tabs?
/// Statistik, Downloads und Einstellungen braucht man selten. Als Tabs
/// würden sie dauerhaft Platz belegen und die drei wichtigen Ziele kleiner
/// machen. Sie liegen deshalb im Profil (runder Knopf oben links).
///
/// Jeder Tab hat seinen EIGENEN Navigator. Dadurch bleiben Tab-Leiste und
/// Mini-Player sichtbar, wenn man z. B. ein Album öffnet, und jeder Tab
/// merkt sich, wo man zuletzt war.
class AppShell extends StatefulWidget {
  const AppShell({super.key});

  /// Erlaubt Seiten, zu einem Tab zu springen (z. B. "Zur Bibliothek" aus
  /// einem leeren Zustand heraus).
  static void goToTab(BuildContext context, int index) {
    context.findAncestorStateOfType<_AppShellState>()?._select(index);
  }

  static const int tabHome = 0;
  static const int tabSearch = 1;
  static const int tabLibrary = 2;

  @override
  State<AppShell> createState() => _AppShellState();
}

class _AppShellState extends State<AppShell> {
  static const _tabCount = 3;

  int _index = 0;

  final _navigatorKeys = List.generate(_tabCount, (_) => GlobalKey<NavigatorState>());

  /// Tabs werden erst gebaut, wenn man sie das erste Mal öffnet. Sonst
  /// würden beim Start alle Seiten gleichzeitig Daten laden.
  final _visited = <int>{0};

  Widget _root(int i) => switch (i) {
    0 => const HomeScreen(),
    1 => const SearchScreen(),
    _ => const LibraryScreen(),
  };

  void _select(int i) {
    if (i == _index) {
      // Erneutes Tippen auf den aktiven Tab springt zu dessen Startseite.
      _navigatorKeys[i].currentState?.popUntil((r) => r.isFirst);
      return;
    }
    setState(() {
      _index = i;
      _visited.add(i);
    });
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    return PopScope(
      // Zurück-Taste: erst innerhalb des Tabs zurückgehen, dann zu Home,
      // und erst von dort die App verlassen.
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (didPop) return;
        final nav = _navigatorKeys[_index].currentState;
        if (nav != null && nav.canPop()) {
          nav.pop();
        } else if (_index != 0) {
          _select(0);
        } else {
          // Auf Home ohne offene Unterseite: App in den Hintergrund. Die
          // Musik läuft im PlaybackService weiter.
          SystemNavigator.pop();
        }
      },
      child: Scaffold(
        // WICHTIG für die Zurück-Taste (im Emulator gefunden):
        // Jeder Tab-Navigator meldet nach oben, ob ER noch eine Seite zum
        // Schließen hat. Steht ein Tab auf seiner Startseite, meldet er
        // "nein". Ohne diesen Listener käme das bei Android an als "Flutter
        // braucht die Zurück-Taste nicht", und Android würde die App bei
        // "Zurück" einfach beenden, ohne uns zu fragen. Wir wollen aber
        // selbst entscheiden (erst zur Startseite des Tabs, dann zu Home).
        // Deshalb fangen wir die Meldung ab und melden stattdessen "ja".
        body: NotificationListener<NavigationNotification>(
          onNotification: (notification) {
            if (notification.canHandlePop) return false;
            const NavigationNotification(canHandlePop: true).dispatch(context);
            return true;
          },
          child: IndexedStack(
            index: _index,
            children: [
              for (var i = 0; i < _tabCount; i++)
                _visited.contains(i)
                    ? Navigator(
                        key: _navigatorKeys[i],
                        onGenerateRoute: (_) => MaterialPageRoute(builder: (_) => _root(i)),
                      )
                    : const SizedBox.shrink(),
            ],
          ),
        ),
        bottomNavigationBar: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            // Schmaler Hinweis, solange die App im Offline-Modus ist.
            ValueListenableBuilder<bool>(
              valueListenable: AppState.instance.offline,
              builder: (context, offline, _) => offline
                  ? Container(
                      width: double.infinity,
                      color: theme.colorScheme.surfaceContainer,
                      padding: const EdgeInsets.symmetric(vertical: 4, horizontal: 12),
                      child: Row(
                        mainAxisAlignment: MainAxisAlignment.center,
                        children: [
                          const Icon(Icons.cloud_off_outlined, size: 14),
                          const SizedBox(width: 6),
                          Flexible(
                            child: Text(
                              l.offlineBanner,
                              style: theme.textTheme.bodySmall,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                        ],
                      ),
                    )
                  : const SizedBox.shrink(),
            ),
            const MiniPlayer(),
            NavigationBar(
              selectedIndex: _index,
              onDestinationSelected: _select,
              // Beschriftung immer zeigen: Bei nur drei Zielen ist Platz
              // genug, und Symbole allein muss man erst lernen.
              labelBehavior: NavigationDestinationLabelBehavior.alwaysShow,
              destinations: [
                NavigationDestination(
                  icon: const Icon(Icons.home_outlined),
                  selectedIcon: const Icon(Icons.home),
                  label: l.home,
                ),
                NavigationDestination(icon: const Icon(Icons.search), label: l.search),
                NavigationDestination(
                  icon: const Icon(Icons.library_music_outlined),
                  selectedIcon: const Icon(Icons.library_music),
                  label: l.library,
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
