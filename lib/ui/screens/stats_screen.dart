import 'package:flutter/material.dart';

import '../../l10n.dart';
import '../../l10n/detail_l10n.dart';
import '../../src/rust/api/stats.dart' as rust;
import '../widgets/async_view.dart';
import '../widgets/cover.dart';
import 'track_list_screen.dart';

/// Statistik-Tab: Hörzeit, Bestenlisten, Aktivität und Verlauf.
///
/// Alle Zahlen kommen aus der LOKALEN Datenbank (eigener Hörverlauf der
/// App). Deshalb funktioniert die Seite offline und unabhängig davon, was
/// der Server mitzählt.
class StatsScreen extends StatefulWidget {
  const StatsScreen({super.key});

  @override
  State<StatsScreen> createState() => _StatsScreenState();
}

class _StatsScreenState extends State<StatsScreen> {
  /// Zeitraum in Tagen, `null` = gesamter Verlauf.
  int? _days = 28;
  late Future<rust.ListeningStats> _future = rust.listeningStats(days: _days);

  void _setDays(int? days) => setState(() {
    _days = days;
    _future = rust.listeningStats(days: days);
  });

  String _duration(BigInt secs) {
    final total = secs.toInt();
    final h = total ~/ 3600;
    final m = (total % 3600) ~/ 60;
    return h > 0 ? '$h h $m min' : '$m min';
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final periods = <(int?, String)>[
      (28, l.period4Weeks),
      (183, l.period6Months),
      (365, l.period1Year),
      (null, l.allTime),
    ];

    return Scaffold(
      appBar: AppBar(
        title: Text(l.stats),
        actions: [
          IconButton(
            tooltip: l.history,
            icon: const Icon(Icons.history),
            onPressed: () => Navigator.of(context).push(
              MaterialPageRoute(
                builder: (_) => TrackListScreen(
                  title: l.history,
                  load: () async => [
                    for (final r in await rust.recentPlays(limit: 200)) r.track,
                  ],
                ),
              ),
            ),
          ),
        ],
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(48),
          child: SizedBox(
            height: 48,
            child: ListView(
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.symmetric(horizontal: 12),
              children: [
                for (final (days, label) in periods)
                  Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 4),
                    child: ChoiceChip(
                      label: Text(label),
                      selected: _days == days,
                      onSelected: (_) => _setDays(days),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
      body: RefreshIndicator(
        onRefresh: () async {
          _setDays(_days);
          await _future;
        },
        child: AsyncView<rust.ListeningStats>(
          future: _future,
          onRetry: () => _setDays(_days),
          builder: (context, s) {
            if (s.totalPlays == 0) {
              // ListView statt Center, damit "Ziehen zum Aktualisieren" geht.
              return ListView(
                children: [
                  const SizedBox(height: 120),
                  EmptyHint(icon: Icons.bar_chart, text: l.statsEmpty),
                ],
              );
            }
            Widget header(String text) => Padding(
              padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
              child: Text(
                text,
                style: theme.textTheme.titleMedium?.copyWith(
                  fontWeight: FontWeight.bold,
                ),
              ),
            );
            return ListView(
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                  child: Row(
                    children: [
                      Expanded(
                        child: _BigNumber(
                          label: l.listeningTime,
                          value: _duration(s.totalSecs),
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: _BigNumber(
                          label: l.plays,
                          value: '${s.totalPlays}',
                        ),
                      ),
                    ],
                  ),
                ),
                if (s.topArtists.isNotEmpty)
                  _TopSection(
                    title: l.topArtists,
                    entries: s.topArtists,
                    showCover: false,
                  ),
                if (s.topTracks.isNotEmpty)
                  _TopSection(title: l.topTracks, entries: s.topTracks),
                if (s.topAlbums.isNotEmpty)
                  _TopSection(title: l.topAlbums, entries: s.topAlbums),
                header(l.byHour),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: _HourBars(hours: s.hours),
                ),
                header(l.activity),
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  child: _Heatmap(
                    days: {for (final d in s.days) d.day: d.plays},
                  ),
                ),
                const SizedBox(height: 24),
              ],
            );
          },
        ),
      ),
    );
  }
}

/// Große Kennzahl in einer Kachel (Hörzeit, Wiedergaben).
class _BigNumber extends StatelessWidget {
  const _BigNumber({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: theme.colorScheme.surfaceContainer,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: theme.textTheme.bodySmall?.copyWith(color: theme.hintColor),
          ),
          const SizedBox(height: 4),
          Text(
            value,
            style: theme.textTheme.headlineSmall?.copyWith(
              fontWeight: FontWeight.bold,
            ),
          ),
        ],
      ),
    );
  }
}

/// Bestenliste mit Überschrift: zeigt 5 Einträge, per Knopf bis zu 10.
class _TopSection extends StatefulWidget {
  const _TopSection({
    required this.title,
    required this.entries,
    this.showCover = true,
  });

  final String title;
  final List<rust.StatEntry> entries;
  final bool showCover;

  @override
  State<_TopSection> createState() => _TopSectionState();
}

class _TopSectionState extends State<_TopSection> {
  bool _expanded = false;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final shown = widget.entries.take(_expanded ? 10 : 5).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 24, 16, 8),
          child: Text(
            widget.title,
            style: theme.textTheme.titleMedium?.copyWith(
              fontWeight: FontWeight.bold,
            ),
          ),
        ),
        for (final (i, e) in shown.indexed)
          _TopTile(rank: i + 1, entry: e, showCover: widget.showCover),
        // Knopf nur, wenn es überhaupt mehr als 5 Einträge gibt.
        if (widget.entries.length > 5)
          Align(
            alignment: Alignment.centerLeft,
            child: Padding(
              padding: const EdgeInsets.only(left: 8),
              child: TextButton(
                onPressed: () => setState(() => _expanded = !_expanded),
                child: Text(_expanded ? l.showLess : l.showAll),
              ),
            ),
          ),
      ],
    );
  }
}

/// Eine Zeile der Bestenliste: Rang, optional Cover, Name und Anzahl.
class _TopTile extends StatelessWidget {
  const _TopTile({
    required this.rank,
    required this.entry,
    this.showCover = true,
  });

  final int rank;
  final rust.StatEntry entry;
  final bool showCover;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return ListTile(
      leading: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          SizedBox(
            width: 24,
            child: Text(
              '$rank',
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.hintColor,
              ),
            ),
          ),
          if (showCover) Cover(coverId: entry.coverArtId, size: 44),
        ],
      ),
      title: Text(entry.name, maxLines: 1, overflow: TextOverflow.ellipsis),
      subtitle: entry.subtitle == null
          ? null
          : Text(entry.subtitle!, maxLines: 1, overflow: TextOverflow.ellipsis),
      trailing: Text(
        L.of(context).playsCount(entry.plays),
        style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
      ),
    );
  }
}

/// 24 Balken: Wann am Tag wird gehört?
///
/// Selbst gezeichnet statt mit einem Diagramm-Paket: Für 24 Rechtecke lohnt
/// sich keine zusätzliche Abhängigkeit.
class _HourBars extends StatelessWidget {
  const _HourBars({required this.hours});

  final List<int> hours;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final max = hours.fold<int>(1, (m, v) => v > m ? v : m);
    return Column(
      children: [
        SizedBox(
          height: 80,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              for (final v in hours)
                Expanded(
                  child: Container(
                    margin: const EdgeInsets.symmetric(horizontal: 1),
                    // Mindesthöhe 2 px, damit auch leere Stunden als
                    // Grundlinie sichtbar bleiben.
                    height: 2 + 78 * v / max,
                    decoration: BoxDecoration(
                      color: v == 0
                          ? theme.colorScheme.onSurface.withValues(alpha: 0.10)
                          : theme.colorScheme.primary,
                      borderRadius: BorderRadius.circular(2),
                    ),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 4),
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            for (final h in const ['0', '6', '12', '18', '24'])
              Text(
                h,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: theme.hintColor,
                ),
              ),
          ],
        ),
      ],
    );
  }
}

/// Aktivitäts-Raster der letzten 16 Wochen: eine Spalte pro Woche, ein
/// Kästchen pro Tag. Je kräftiger, desto mehr wurde an dem Tag gehört.
class _Heatmap extends StatelessWidget {
  const _Heatmap({required this.days});

  /// "JJJJ-MM-TT" -> Anzahl Wiedergaben.
  final Map<String, int> days;

  static const _weeks = 16;

  String _key(DateTime d) =>
      '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final today = DateTime.now();
    // Montag der aktuellen Woche, dann 15 Wochen zurück.
    final monday = DateTime(
      today.year,
      today.month,
      today.day,
    ).subtract(Duration(days: today.weekday - 1));
    final start = monday.subtract(const Duration(days: 7 * (_weeks - 1)));
    final max = days.values.fold<int>(1, (m, v) => v > m ? v : m);

    return AspectRatio(
      aspectRatio: _weeks / 7,
      child: Row(
        children: [
          for (var w = 0; w < _weeks; w++)
            Expanded(
              child: Column(
                children: [
                  for (var d = 0; d < 7; d++)
                    Expanded(
                      child: Builder(
                        builder: (context) {
                          // DateTime(y, m, tag + n) rechnet Monatswechsel und
                          // Sommerzeit korrekt; Duration(days: n) täte das nicht.
                          final date = DateTime(
                            start.year,
                            start.month,
                            start.day + w * 7 + d,
                          );
                          final future = date.isAfter(today);
                          final n = days[_key(date)] ?? 0;
                          return Container(
                            margin: const EdgeInsets.all(1.5),
                            decoration: BoxDecoration(
                              color: future
                                  ? Colors.transparent
                                  : n == 0
                                  // Leicht getönt (aus der Textfarbe), damit
                                  // leere Kästchen auch im hellen Design
                                  // sichtbar sind.
                                  ? theme.colorScheme.onSurface.withValues(
                                      alpha: 0.08,
                                    )
                                  : theme.colorScheme.primary.withValues(
                                      alpha: 0.25 + 0.75 * n / max,
                                    ),
                              borderRadius: BorderRadius.circular(3),
                            ),
                          );
                        },
                      ),
                    ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}
