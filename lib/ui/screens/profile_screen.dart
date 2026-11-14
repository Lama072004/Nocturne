import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/session.dart';
import '../../src/rust/api/stats.dart' as stats;
import 'downloads_screen.dart';
import 'equalizer_screen.dart';
import 'settings_screen.dart';
import 'stats_screen.dart';
import 'track_list_screen.dart';

/// Profilseite: alles, was zum Nutzer gehört und kein eigener Tab sein soll.
///
/// Aufbau wie bei Spotify (Profilbild oben links öffnet diese Seite):
/// Statistik zuerst, dann Verlauf, Downloads, Klang, Einstellungen.
/// So bleiben unten nur die drei Tabs, die man ständig braucht.
class ProfileScreen extends StatefulWidget {
  const ProfileScreen({super.key});

  @override
  State<ProfileScreen> createState() => _ProfileScreenState();
}

class _ProfileScreenState extends State<ProfileScreen> {
  /// Kurzfassung der Statistik für die Karte oben (letzte 4 Wochen).
  late final Future<stats.ListeningStats> _summary = stats.listeningStats(days: 28);

  /// "3 h 20 min" bzw. "45 min" aus Sekunden.
  static String formatListeningTime(int totalSeconds) {
    final h = totalSeconds ~/ 3600;
    final m = (totalSeconds % 3600) ~/ 60;
    return h > 0 ? '$h h $m min' : '$m min';
  }

  void _open(Widget screen) {
    Navigator.of(context).push(MaterialPageRoute(builder: (_) => screen));
  }

  Future<void> _logout() async {
    final l = L.of(context);
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(l.logout),
        content: Text(l.logoutConfirm),
        actions: [
          TextButton(onPressed: () => Navigator.pop(context, false), child: Text(l.cancel)),
          FilledButton(onPressed: () => Navigator.pop(context, true), child: Text(l.logout)),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    final nav = Navigator.of(context, rootNavigator: true);
    await rust.logout();
    AppState.instance.loggedIn.value = false;
    // Zurück bis zur Wurzel, dort erscheint dann die Anmeldung.
    nav.popUntil((r) => r.isFirst);
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final info = sessionInfo();
    final name = (info?.username.isNotEmpty ?? false) ? info!.username : l.profile;

    return Scaffold(
      appBar: AppBar(title: Text(l.profile)),
      body: ListView(
        children: [
          // ---------------- Kopf ----------------
          Padding(
            padding: const EdgeInsets.fromLTRB(16, 8, 16, 16),
            child: Row(
              children: [
                CircleAvatar(
                  radius: 32,
                  backgroundColor: theme.colorScheme.primary,
                  child: Text(
                    name.characters.first.toUpperCase(),
                    style: theme.textTheme.headlineMedium?.copyWith(
                      color: theme.colorScheme.onPrimary,
                      fontWeight: FontWeight.bold,
                    ),
                  ),
                ),
                const SizedBox(width: 16),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.titleLarge?.copyWith(fontWeight: FontWeight.bold),
                      ),
                      if (info != null)
                        Text(
                          // Nur der Rechnername, ohne "https://": kürzer und
                          // für die Anzeige das, was man wiedererkennt.
                          Uri.tryParse(info.serverUrl)?.host ?? info.serverUrl,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                        ),
                      const SizedBox(height: 4),
                      // Online/Offline direkt am Profil: Man sieht sofort,
                      // warum evtl. nur Downloads angezeigt werden.
                      ValueListenableBuilder<bool>(
                        valueListenable: AppState.instance.offline,
                        builder: (context, offline, _) => Row(
                          children: [
                            Icon(
                              offline ? Icons.cloud_off_outlined : Icons.cloud_done_outlined,
                              size: 16,
                              color: offline ? theme.hintColor : theme.colorScheme.primary,
                            ),
                            const SizedBox(width: 6),
                            Text(
                              offline ? l.offline : l.online,
                              style: theme.textTheme.bodySmall,
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),

          // ---------------- Statistik-Karte ----------------
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 16),
            child: Material(
              color: theme.colorScheme.surfaceContainer,
              borderRadius: BorderRadius.circular(12),
              clipBehavior: Clip.antiAlias,
              child: InkWell(
                onTap: () => _open(const StatsScreen()),
                child: Padding(
                  padding: const EdgeInsets.all(16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              l.yourStats,
                              style: theme.textTheme.titleMedium?.copyWith(
                                fontWeight: FontWeight.bold,
                              ),
                            ),
                          ),
                          Text(
                            l.last4Weeks,
                            style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
                          ),
                          const Icon(Icons.chevron_right),
                        ],
                      ),
                      const SizedBox(height: 12),
                      FutureBuilder<stats.ListeningStats>(
                        future: _summary,
                        builder: (context, snap) {
                          final s = snap.data;
                          // Solange geladen wird (oder bei einem Fehler)
                          // Striche zeigen: Die Karte springt dann nicht.
                          final time = s == null ? '–' : formatListeningTime(s.totalSecs.toInt());
                          final plays = s == null ? '–' : '${s.totalPlays}';
                          final top = (s == null || s.topArtists.isEmpty)
                              ? null
                              : s.topArtists.first.name;
                          return Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Row(
                                children: [
                                  Expanded(child: _Figure(value: time, label: l.listeningTime)),
                                  Expanded(child: _Figure(value: plays, label: l.plays)),
                                ],
                              ),
                              if (top != null) ...[
                                const SizedBox(height: 12),
                                Text(
                                  '${l.topArtists}: $top',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: theme.textTheme.bodyMedium,
                                ),
                              ],
                            ],
                          );
                        },
                      ),
                    ],
                  ),
                ),
              ),
            ),
          ),
          const SizedBox(height: 8),

          // ---------------- Einträge ----------------
          ListTile(
            leading: const Icon(Icons.history),
            title: Text(l.history),
            onTap: () => _open(
              TrackListScreen(
                title: l.history,
                load: () async => [for (final r in await stats.recentPlays(limit: 200)) r.track],
              ),
            ),
          ),
          ListTile(
            leading: const Icon(Icons.download_outlined),
            title: Text(l.manageDownloads),
            onTap: () => _open(const DownloadsScreen()),
          ),
          ListTile(
            leading: const Icon(Icons.graphic_eq),
            title: Text(l.sound),
            subtitle: Text('${l.equalizer}, ${l.compressor}, ${l.replayGain}'),
            onTap: () => _open(const EqualizerScreen()),
          ),
          ListTile(
            leading: const Icon(Icons.settings_outlined),
            title: Text(l.settings),
            onTap: () => _open(const SettingsScreen()),
          ),
          const Divider(),
          ListTile(
            leading: const Icon(Icons.logout),
            title: Text(l.logout),
            onTap: _logout,
          ),
          const SizedBox(height: 24),
        ],
      ),
    );
  }
}

/// Große Zahl mit kleiner Beschriftung darunter.
class _Figure extends StatelessWidget {
  const _Figure({required this.value, required this.label});

  final String value;
  final String label;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(value, style: theme.textTheme.headlineSmall?.copyWith(fontWeight: FontWeight.bold)),
        Text(label, style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor)),
      ],
    );
  }
}
