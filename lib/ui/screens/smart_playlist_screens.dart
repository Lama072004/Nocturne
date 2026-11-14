import 'package:flutter/material.dart';

import '../../downloads/auto_sync.dart';
import '../../l10n.dart';
import '../../src/rust/api/smart.dart';
import '../actions.dart';
import '../widgets/async_view.dart';
import 'track_list_screen.dart';

// ---------------------------------------------------------------------------
// Hilfen: Welche Vergleiche passen zu welchem Feld?
// ---------------------------------------------------------------------------

enum _FieldType { text, number, yesNo }

_FieldType _typeOf(RuleField f) => switch (f) {
  RuleField.title ||
  RuleField.artist ||
  RuleField.album ||
  RuleField.genre => _FieldType.text,
  RuleField.starred || RuleField.downloaded => _FieldType.yesNo,
  _ => _FieldType.number,
};

/// Nur sinnvolle Vergleiche anbieten: "größer als" bei Text oder "enthält"
/// bei Zahlen würde nie etwas finden und nur verwirren.
List<RuleOp> _opsFor(RuleField f) => switch (_typeOf(f)) {
  _FieldType.text => const [
    RuleOp.contains,
    RuleOp.notContains,
    RuleOp.is_,
    RuleOp.isNot,
  ],
  _FieldType.number => const [
    RuleOp.greater,
    RuleOp.less,
    RuleOp.is_,
    RuleOp.isNot,
  ],
  _FieldType.yesNo => const [RuleOp.isTrue, RuleOp.isFalse],
};

SmartPlaylist _copy(
  SmartPlaylist p, {
  String? name,
  RuleSet? rules,
  SmartSort? sort,
  int? limitCount,
  bool clearLimitCount = false,
  int? limitMinutes,
  bool clearLimitMinutes = false,
  bool? mirrorToServer,
  bool? autoDownload,
}) => SmartPlaylist(
  id: p.id,
  name: name ?? p.name,
  rules: rules ?? p.rules,
  sort: sort ?? p.sort,
  limitCount: clearLimitCount ? null : (limitCount ?? p.limitCount),
  limitMinutes: clearLimitMinutes ? null : (limitMinutes ?? p.limitMinutes),
  mirrorToServer: mirrorToServer ?? p.mirrorToServer,
  serverPlaylistId: p.serverPlaylistId,
  autoDownload: autoDownload ?? p.autoDownload,
);

/// Fertige Vorlagen. Sie sind normale Smart-Playlists: Nach dem Anlegen
/// kann man jede Regel frei ändern.
List<SmartPlaylist> _templates(L l) {
  SmartPlaylist make(
    String name,
    List<Rule> rules,
    SmartSort sort,
    int limit,
  ) => SmartPlaylist(
    id: '',
    name: name,
    rules: RuleSet(matchAll: true, rules: rules, groups: const []),
    sort: sort,
    limitCount: limit,
    limitMinutes: null,
    mirrorToServer: false,
    serverPlaylistId: null,
    autoDownload: false,
  );
  Rule r(RuleField f, RuleOp op, [String v = '']) =>
      Rule(field: f, op: op, value: v);
  return [
    make(
      l.tplForgottenFavorites,
      [
        r(RuleField.starred, RuleOp.isTrue),
        r(RuleField.daysSincePlayed, RuleOp.greater, '60'),
      ],
      SmartSort.random,
      50,
    ),
    make(
      l.tplNewlyAdded,
      [r(RuleField.daysSinceAdded, RuleOp.less, '30')],
      SmartSort.addedNewest,
      100,
    ),
    make(
      l.tplMostPlayed,
      [r(RuleField.playCount, RuleOp.greater, '4')],
      SmartSort.mostPlayed,
      50,
    ),
    make(
      l.tplNeverPlayed,
      [r(RuleField.playCount, RuleOp.is_, '0')],
      SmartSort.random,
      100,
    ),
    make(
      l.tplTopRated,
      [r(RuleField.rating, RuleOp.greater, '3')],
      SmartSort.bestRated,
      100,
    ),
  ];
}

// ---------------------------------------------------------------------------
// Liste der Smart-Playlists (Bereich im Bibliothek-Tab)
// ---------------------------------------------------------------------------

class SmartPlaylistList extends StatefulWidget {
  const SmartPlaylistList({super.key});

  @override
  State<SmartPlaylistList> createState() => _SmartPlaylistListState();
}

class _SmartPlaylistListState extends State<SmartPlaylistList> {
  late Future<(List<SmartPlaylist>, LibrarySyncInfo)> _future = _load();
  bool _syncing = false;

  Future<(List<SmartPlaylist>, LibrarySyncInfo)> _load() async =>
      (await listSmartPlaylists(), await librarySyncInfo());

  void _reload() => setState(() {
    _future = _load();
  });

  /// Holt die Titel-Liste vom Server in die lokale Datenbank.
  Future<void> _sync() async {
    final l = L.of(context);
    setState(() => _syncing = true);
    try {
      final n = await syncLibrary();
      if (mounted) showToast(context, l.librarySynced(n));
    } catch (e) {
      if (mounted) showToast(context, '$e');
    } finally {
      if (mounted) {
        setState(() => _syncing = false);
        _reload();
      }
    }
  }

  Future<void> _create() async {
    final l = L.of(context);
    final template = await showModalBottomSheet<SmartPlaylist>(
      context: context,
      useRootNavigator: true,
      showDragHandle: true,
      builder: (context) => SafeArea(
        child: ListView(
          shrinkWrap: true,
          children: [
            ListTile(
              leading: const Icon(Icons.add),
              title: Text(l.smartEmpty),
              onTap: () => Navigator.of(
                context,
              ).pop(_templates(l).first.withEmptyRules(l)),
            ),
            const Divider(),
            for (final t in _templates(l))
              ListTile(
                leading: const Icon(Icons.auto_awesome_outlined),
                title: Text(t.name),
                onTap: () => Navigator.of(context).pop(t),
              ),
          ],
        ),
      ),
    );
    if (template == null || !mounted) return;
    await _edit(template);
  }

  Future<void> _edit(SmartPlaylist playlist) async {
    await Navigator.of(context).push(
      MaterialPageRoute(builder: (_) => SmartPlaylistEditor(initial: playlist)),
    );
    _reload();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    return AsyncView<(List<SmartPlaylist>, LibrarySyncInfo)>(
      future: _future,
      onRetry: _reload,
      builder: (context, data) {
        final (playlists, info) = data;
        final synced = info.lastSyncMs == null
            ? l.libraryNeverSynced
            : l.libraryTracks(info.trackCount);
        return ListView(
          children: [
            // Stand der lokalen Titel-Kopie. Ohne sie haben die Regeln
            // nichts, worauf sie rechnen können – deshalb ganz oben.
            ListTile(
              leading: _syncing
                  ? const SizedBox(
                      width: 24,
                      height: 24,
                      child: CircularProgressIndicator(strokeWidth: 2),
                    )
                  : const Icon(Icons.sync),
              title: Text(synced),
              subtitle: Text(l.librarySyncHint),
              onTap: _syncing ? null : _sync,
            ),
            ListTile(
              leading: const Icon(Icons.add),
              title: Text(l.newSmartPlaylist),
              onTap: _create,
            ),
            if (playlists.isNotEmpty) const Divider(),
            for (final p in playlists)
              ListTile(
                leading: Icon(
                  Icons.auto_awesome,
                  color: theme.colorScheme.primary,
                ),
                title: Text(
                  p.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                trailing: IconButton(
                  tooltip: l.edit,
                  icon: const Icon(Icons.tune),
                  onPressed: () => _edit(p),
                ),
                onTap: () => Navigator.of(context).push(
                  MaterialPageRoute(
                    builder: (_) => TrackListScreen(
                      title: p.name,
                      // Bei jedem Öffnen neu auswerten: Die Liste soll den
                      // aktuellen Stand zeigen (und "Zufällig" neu mischen).
                      load: () => evaluateSmartPlaylist(id: p.id),
                    ),
                  ),
                ),
              ),
            const SizedBox(height: 24),
          ],
        );
      },
    );
  }
}

extension on SmartPlaylist {
  /// Leere Playlist mit dem Standardnamen.
  SmartPlaylist withEmptyRules(L l) => SmartPlaylist(
    id: '',
    name: l.newSmartPlaylist,
    rules: const RuleSet(matchAll: true, rules: [], groups: []),
    sort: SmartSort.random,
    limitCount: 50,
    limitMinutes: null,
    mirrorToServer: false,
    serverPlaylistId: null,
    autoDownload: false,
  );
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

class SmartPlaylistEditor extends StatefulWidget {
  const SmartPlaylistEditor({super.key, required this.initial});

  final SmartPlaylist initial;

  @override
  State<SmartPlaylistEditor> createState() => _SmartPlaylistEditorState();
}

class _SmartPlaylistEditorState extends State<SmartPlaylistEditor> {
  late SmartPlaylist _p = widget.initial;
  late final _name = TextEditingController(text: widget.initial.name);

  /// Anzahl der Treffer der aktuellen Regeln (`null` = noch nicht berechnet).
  int? _matches;

  @override
  void initState() {
    super.initState();
    _preview();
  }

  @override
  void dispose() {
    _name.dispose();
    super.dispose();
  }

  void _set(SmartPlaylist p) {
    setState(() => _p = p);
    _preview();
  }

  void _setRules(RuleSet rules) => _set(_copy(_p, rules: rules));

  /// Zeigt sofort, wie viele Titel die Regeln treffen. So merkt man beim
  /// Bauen, ob eine Regel zu streng ist, statt erst nach dem Speichern.
  Future<void> _preview() async {
    try {
      // Für die Zählung ohne Limit auswerten, sonst stünde dort immer nur
      // der Wert des Limits.
      final all = await previewSmartPlaylist(
        playlist: _copy(_p, clearLimitCount: true, clearLimitMinutes: true),
      );
      if (mounted) setState(() => _matches = all.length);
    } catch (_) {
      if (mounted) setState(() => _matches = null);
    }
  }

  Future<void> _save() async {
    final l = L.of(context);
    final name = _name.text.trim();
    if (name.isEmpty) {
      showToast(context, l.playlistName);
      return;
    }
    try {
      await saveSmartPlaylist(playlist: _copy(_p, name: name));
      // Falls "automatisch herunterladen" an ist, gleich loslegen.
      AutoSync.run();
      if (mounted) Navigator.of(context).pop();
    } catch (e) {
      if (mounted) showToast(context, '$e');
    }
  }

  Future<void> _delete() async {
    await deleteSmartPlaylist(id: _p.id);
    if (mounted) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final rules = _p.rules;

    return Scaffold(
      appBar: AppBar(
        title: Text(l.smartPlaylist),
        actions: [
          if (_p.id.isNotEmpty)
            IconButton(
              tooltip: l.delete,
              icon: const Icon(Icons.delete_outline),
              onPressed: _delete,
            ),
          TextButton(onPressed: _save, child: Text(l.save)),
        ],
      ),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
        children: [
          TextField(
            controller: _name,
            decoration: InputDecoration(labelText: l.playlistName),
          ),
          const SizedBox(height: 20),

          // ---------------- Regeln ----------------
          _MatchToggle(
            matchAll: rules.matchAll,
            onChanged: (v) => _setRules(
              RuleSet(matchAll: v, rules: rules.rules, groups: rules.groups),
            ),
          ),
          for (var i = 0; i < rules.rules.length; i++)
            _RuleRow(
              rule: rules.rules[i],
              onChanged: (r) => _setRules(
                RuleSet(
                  matchAll: rules.matchAll,
                  rules: [...rules.rules]..[i] = r,
                  groups: rules.groups,
                ),
              ),
              onRemove: () => _setRules(
                RuleSet(
                  matchAll: rules.matchAll,
                  rules: [...rules.rules]..removeAt(i),
                  groups: rules.groups,
                ),
              ),
            ),
          // Untergruppen: je ein eigener Kasten mit eigenem UND/ODER.
          for (var g = 0; g < rules.groups.length; g++)
            _GroupCard(
              group: rules.groups[g],
              onChanged: (group) => _setRules(
                RuleSet(
                  matchAll: rules.matchAll,
                  rules: rules.rules,
                  groups: [...rules.groups]..[g] = group,
                ),
              ),
              onRemove: () => _setRules(
                RuleSet(
                  matchAll: rules.matchAll,
                  rules: rules.rules,
                  groups: [...rules.groups]..removeAt(g),
                ),
              ),
            ),
          Wrap(
            spacing: 8,
            children: [
              OutlinedButton.icon(
                icon: const Icon(Icons.add),
                label: Text(l.addRule),
                onPressed: () => _setRules(
                  RuleSet(
                    matchAll: rules.matchAll,
                    rules: [
                      ...rules.rules,
                      const Rule(
                        field: RuleField.genre,
                        op: RuleOp.contains,
                        value: '',
                      ),
                    ],
                    groups: rules.groups,
                  ),
                ),
              ),
              OutlinedButton.icon(
                icon: const Icon(Icons.account_tree_outlined),
                label: Text(l.addGroup),
                onPressed: () => _setRules(
                  RuleSet(
                    matchAll: rules.matchAll,
                    rules: rules.rules,
                    groups: [
                      ...rules.groups,
                      // Eine neue Gruppe ist "eine reicht": Das ist der
                      // übliche Grund, überhaupt eine Gruppe anzulegen.
                      const RuleGroup(
                        matchAll: false,
                        rules: [
                          Rule(
                            field: RuleField.genre,
                            op: RuleOp.contains,
                            value: '',
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text(
            _matches == null ? '' : l.smartMatches(_matches!),
            style: theme.textTheme.bodyMedium?.copyWith(
              color: theme.colorScheme.primary,
            ),
          ),

          // ---------------- Sortierung und Limit ----------------
          const Divider(height: 32),
          Row(
            children: [
              Expanded(child: Text(l.sortBy)),
              DropdownButton<SmartSort>(
                value: _p.sort,
                underline: const SizedBox.shrink(),
                items: [
                  for (final s in SmartSort.values)
                    DropdownMenuItem(value: s, child: Text(l.smartSortName(s))),
                ],
                onChanged: (s) => _set(_copy(_p, sort: s)),
              ),
            ],
          ),
          _NumberField(
            label: l.limitCount,
            value: _p.limitCount,
            onChanged: (v) => _set(
              v == null
                  ? _copy(_p, clearLimitCount: true)
                  : _copy(_p, limitCount: v),
            ),
          ),
          _NumberField(
            label: l.limitMinutes,
            value: _p.limitMinutes,
            onChanged: (v) => _set(
              v == null
                  ? _copy(_p, clearLimitMinutes: true)
                  : _copy(_p, limitMinutes: v),
            ),
          ),

          // ---------------- Optionen ----------------
          const Divider(height: 32),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l.mirrorToServer),
            subtitle: Text(l.mirrorToServerHint),
            value: _p.mirrorToServer,
            onChanged: (v) => _set(_copy(_p, mirrorToServer: v)),
          ),
          SwitchListTile(
            contentPadding: EdgeInsets.zero,
            title: Text(l.autoDownload),
            subtitle: Text(l.autoDownloadHint),
            value: _p.autoDownload,
            onChanged: (v) => _set(_copy(_p, autoDownload: v)),
          ),
        ],
      ),
    );
  }
}

/// Umschalter "alle müssen zutreffen" / "eine reicht".
class _MatchToggle extends StatelessWidget {
  const _MatchToggle({required this.matchAll, required this.onChanged});

  final bool matchAll;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: SegmentedButton<bool>(
        segments: [
          ButtonSegment(value: true, label: Text(l.matchAll)),
          ButtonSegment(value: false, label: Text(l.matchAny)),
        ],
        selected: {matchAll},
        onSelectionChanged: (s) => onChanged(s.first),
      ),
    );
  }
}

class _GroupCard extends StatelessWidget {
  const _GroupCard({
    required this.group,
    required this.onChanged,
    required this.onRemove,
  });

  final RuleGroup group;
  final ValueChanged<RuleGroup> onChanged;
  final VoidCallback onRemove;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    return Card(
      margin: const EdgeInsets.symmetric(vertical: 8),
      color: theme.colorScheme.surfaceContainer,
      elevation: 0,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 8, 4, 8),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: _MatchToggle(
                    matchAll: group.matchAll,
                    onChanged: (v) =>
                        onChanged(RuleGroup(matchAll: v, rules: group.rules)),
                  ),
                ),
                IconButton(icon: const Icon(Icons.close), onPressed: onRemove),
              ],
            ),
            for (var i = 0; i < group.rules.length; i++)
              _RuleRow(
                rule: group.rules[i],
                onChanged: (r) => onChanged(
                  RuleGroup(
                    matchAll: group.matchAll,
                    rules: [...group.rules]..[i] = r,
                  ),
                ),
                onRemove: () => onChanged(
                  RuleGroup(
                    matchAll: group.matchAll,
                    rules: [...group.rules]..removeAt(i),
                  ),
                ),
              ),
            TextButton.icon(
              icon: const Icon(Icons.add),
              label: Text(l.addRule),
              onPressed: () => onChanged(
                RuleGroup(
                  matchAll: group.matchAll,
                  rules: [
                    ...group.rules,
                    const Rule(
                      field: RuleField.genre,
                      op: RuleOp.contains,
                      value: '',
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Eine Regel: Feld, Vergleich, Wert.
class _RuleRow extends StatefulWidget {
  const _RuleRow({
    required this.rule,
    required this.onChanged,
    required this.onRemove,
  });

  final Rule rule;
  final ValueChanged<Rule> onChanged;
  final VoidCallback onRemove;

  @override
  State<_RuleRow> createState() => _RuleRowState();
}

class _RuleRowState extends State<_RuleRow> {
  late final _value = TextEditingController(text: widget.rule.value);

  @override
  void didUpdateWidget(_RuleRow old) {
    super.didUpdateWidget(old);
    // Der Wert wurde von außen geändert (z. B. Regel davor gelöscht, diese
    // Zeile zeigt jetzt eine andere Regel) -> Textfeld nachziehen. Beim
    // eigenen Tippen ist der Text schon gleich, dann passiert nichts.
    if (widget.rule.value != _value.text) _value.text = widget.rule.value;
  }

  @override
  void dispose() {
    _value.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final rule = widget.rule;
    final type = _typeOf(rule.field);
    final ops = _opsFor(rule.field);

    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Expanded(
            flex: 5,
            child: DropdownButton<RuleField>(
              isExpanded: true,
              value: rule.field,
              underline: const SizedBox.shrink(),
              items: [
                for (final f in RuleField.values)
                  DropdownMenuItem(value: f, child: Text(l.ruleFieldName(f))),
              ],
              onChanged: (f) {
                if (f == null) return;
                // Passt der bisherige Vergleich nicht zum neuen Feld, den
                // ersten passenden nehmen; ebenso den Wert leeren, wenn sich
                // die Art des Felds ändert (Text <-> Zahl <-> Ja/Nein).
                final newOps = _opsFor(f);
                final sameType = _typeOf(f) == type;
                widget.onChanged(
                  Rule(
                    field: f,
                    op: newOps.contains(rule.op) ? rule.op : newOps.first,
                    value: sameType ? rule.value : '',
                  ),
                );
              },
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            flex: 5,
            child: DropdownButton<RuleOp>(
              isExpanded: true,
              value: ops.contains(rule.op) ? rule.op : ops.first,
              underline: const SizedBox.shrink(),
              items: [
                for (final o in ops)
                  DropdownMenuItem(value: o, child: Text(l.ruleOpName(o))),
              ],
              onChanged: (o) => widget.onChanged(
                Rule(field: rule.field, op: o ?? rule.op, value: rule.value),
              ),
            ),
          ),
          // Ja/Nein-Felder brauchen keinen Wert.
          if (type != _FieldType.yesNo) ...[
            const SizedBox(width: 6),
            Expanded(
              flex: 4,
              child: TextField(
                controller: _value,
                keyboardType: type == _FieldType.number
                    ? TextInputType.number
                    : TextInputType.text,
                decoration: const InputDecoration(isDense: true),
                onChanged: (v) => widget.onChanged(
                  Rule(field: rule.field, op: rule.op, value: v),
                ),
              ),
            ),
          ],
          IconButton(
            icon: const Icon(Icons.close, size: 20),
            onPressed: widget.onRemove,
          ),
        ],
      ),
    );
  }
}

/// Zahlenfeld, das auch leer sein darf (leer = kein Limit).
class _NumberField extends StatefulWidget {
  const _NumberField({
    required this.label,
    required this.value,
    required this.onChanged,
  });

  final String label;
  final int? value;
  final ValueChanged<int?> onChanged;

  @override
  State<_NumberField> createState() => _NumberFieldState();
}

class _NumberFieldState extends State<_NumberField> {
  late final _controller = TextEditingController(
    text: widget.value?.toString() ?? '',
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Row(
      children: [
        Expanded(child: Text(widget.label)),
        SizedBox(
          width: 90,
          child: TextField(
            controller: _controller,
            keyboardType: TextInputType.number,
            textAlign: TextAlign.end,
            decoration: InputDecoration(
              isDense: true,
              hintText: L.of(context).unlimited,
            ),
            // Ungültige Eingaben (Buchstaben) zählen wie "leer".
            onChanged: (v) => widget.onChanged(int.tryParse(v.trim())),
          ),
        ),
      ],
    );
  }
}
