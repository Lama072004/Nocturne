import 'dart:async';

import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/search_l10n.dart';
import '../../prefs.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../widgets/album_widgets.dart';
import '../widgets/async_view.dart';
import '../widgets/profile_button.dart';
import '../widgets/track_tile.dart';
import 'track_list_screen.dart';

/// Welche Abschnitte die Ergebnisse gerade zeigen (Chips unter dem Suchfeld).
enum _Filter { all, tracks, albums, artists }

/// Schlüssel im Merkzettel für die zuletzt gesuchten Begriffe.
const _kRecentKey = 'search.recent';
const _kMaxRecent = 10;

/// Feste kleine Farbpalette für die Genre-Kacheln (Auswahl nach Index).
const _genreColors = <Color>[
  Color(0xFF7B4FD6),
  Color(0xFF1E88A8),
  Color(0xFFC2527A),
  Color(0xFF3F8F5B),
  Color(0xFFC0792E),
  Color(0xFF4F6BD6),
];

/// Suche-Tab.
///
/// Ohne Eingabe: "Zuletzt gesucht" und Genre-Kacheln. Mit Eingabe: Treffer in
/// Titel, Alben und Künstler. Die lokale Kopie der Bibliothek antwortet sofort
/// (auch offline); die Server-Suche kommt hinzu, sobald sie antwortet.
class SearchScreen extends StatefulWidget {
  const SearchScreen({super.key});

  @override
  State<SearchScreen> createState() => _SearchScreenState();
}

class _SearchScreenState extends State<SearchScreen> {
  final _controller = TextEditingController();
  final _focus = FocusNode();
  Timer? _debounce;

  /// Aktuell gesuchter (getrimmter) Begriff; leer = Startansicht.
  String _query = '';
  _Filter _filter = _Filter.all;
  List<Track> _tracks = const [];
  List<Album> _albums = const [];
  List<Artist> _artists = const [];
  bool _loading = false;

  /// Zählt die Anfragen. Eine ältere, langsame Antwort wird verworfen, wenn
  /// inzwischen eine neuere gestartet wurde.
  int _requestId = 0;

  /// Genres einmalig laden (lokale Kopie; leer, wenn sie fehlt).
  late final Future<List<local.GenreCount>> _genres = _loadGenres();

  Future<List<local.GenreCount>> _loadGenres() async {
    try {
      final all = await local.localGenres();
      return all.take(30).toList();
    } catch (_) {
      return const [];
    }
  }

  @override
  void initState() {
    super.initState();
    // Falls der Schlüssel noch nicht im Speicher ist, nachladen.
    Prefs.instance.load(_kRecentKey);
  }

  @override
  void dispose() {
    _debounce?.cancel();
    _controller.dispose();
    _focus.dispose();
    super.dispose();
  }

  // ---------------------------------------------------------------- Eingabe

  void _onChanged(String text) {
    _debounce?.cancel();
    final q = text.trim();
    if (q.isEmpty) {
      // Sofort zurück zur Startansicht; laufende Anfragen entwerten.
      _requestId++;
      setState(() {
        _query = '';
        _loading = false;
        _tracks = const [];
        _albums = const [];
        _artists = const [];
      });
      return;
    }
    // Neu zeichnen, damit das × im Suchfeld erscheint.
    setState(() {});
    // 250 ms nach dem letzten Tastendruck, sonst ginge pro Buchstabe eine
    // Anfrage hinaus.
    _debounce = Timer(const Duration(milliseconds: 250), () => _search(q));
  }

  /// Setzt den Text (z. B. aus "Zuletzt gesucht") und sucht ohne Wartezeit.
  /// Spielt bewusst nichts ab.
  void _setQueryAndSearch(String q) {
    _controller.value = TextEditingValue(
      text: q,
      selection: TextSelection.collapsed(offset: q.length),
    );
    _debounce?.cancel();
    _search(q.trim());
  }

  Future<void> _search(String query) async {
    final id = ++_requestId;
    setState(() {
      _query = query;
      _loading = true;
    });

    // 1. Lokal: sofort da und offline nutzbar.
    SearchResult? localResult;
    try {
      localResult = await local.localSearch(query: query);
    } catch (_) {
      // Fehlt die lokale Kopie, bleibt die Server-Suche.
    }
    if (!mounted || id != _requestId) return;
    if (localResult != null) _apply(localResult, null);

    // 2. Server, nur online. Ein Fehler hier lässt die lokalen Treffer stehen.
    if (!AppState.instance.offline.value) {
      SearchResult? remote;
      try {
        remote = await rust.search(query: query);
      } catch (_) {}
      if (!mounted || id != _requestId) return;
      if (remote != null) _apply(localResult, remote);
    }
    if (!mounted || id != _requestId) return;
    setState(() => _loading = false);
  }

  /// Führt lokale und Server-Treffer zusammen: lokale zuerst, ohne Doppelte
  /// (gleiche `id`).
  void _apply(SearchResult? a, SearchResult? b) {
    List<T> merge<T>(List<T>? x, List<T>? y, String Function(T) idOf) {
      final seen = <String>{};
      return [
        for (final item in [...?x, ...?y])
          if (seen.add(idOf(item))) item,
      ];
    }

    setState(() {
      _tracks = merge<Track>(a?.tracks, b?.tracks, (t) => t.id);
      _albums = merge<Album>(a?.albums, b?.albums, (t) => t.id);
      _artists = merge<Artist>(a?.artists, b?.artists, (t) => t.id);
    });
  }

  // ------------------------------------------------------- Zuletzt gesucht

  /// Merkt den aktuellen Begriff: neueste zuerst, ohne Doppelte (Groß-/
  /// Kleinschreibung egal), höchstens [_kMaxRecent].
  void _rememberQuery() {
    final q = _query.trim();
    if (q.isEmpty) return;
    final list = Prefs.instance.getList(_kRecentKey);
    final next = [
      q,
      ...list.where((e) => e.toLowerCase() != q.toLowerCase()),
    ].take(_kMaxRecent).toList();
    Prefs.instance.setList(_kRecentKey, next);
  }

  void _removeRecent(String q) {
    final list = Prefs.instance.getList(_kRecentKey);
    Prefs.instance.setList(_kRecentKey, [...list.where((e) => e != q)]);
  }

  // ------------------------------------------------------------------ Bau

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    final theme = Theme.of(context);
    final hasText = _controller.text.isNotEmpty;

    return Scaffold(
      appBar: AppBar(
        leading: const ProfileButton(),
        leadingWidth: 60,
        titleSpacing: 0,
        title: Padding(
          padding: const EdgeInsets.only(right: 16),
          child: TextField(
            controller: _controller,
            focusNode: _focus,
            textInputAction: TextInputAction.search,
            onChanged: _onChanged,
            // Tastatur bestätigt: Begriff merken und ohne Wartezeit suchen.
            onSubmitted: (v) {
              _debounce?.cancel();
              final q = v.trim();
              if (q.isEmpty) return;
              _search(q);
              _rememberQuery();
            },
            decoration: InputDecoration(
              hintText: l.searchHint,
              filled: true,
              fillColor: theme.colorScheme.surfaceContainerHigh,
              isDense: true,
              contentPadding: const EdgeInsets.symmetric(vertical: 12),
              prefixIcon: const Icon(Icons.search),
              suffixIcon: hasText
                  ? IconButton(
                      icon: const Icon(Icons.close),
                      tooltip: l.searchClearField,
                      onPressed: () {
                        _controller.clear();
                        _onChanged('');
                        _focus.requestFocus();
                      },
                    )
                  : null,
              border: OutlineInputBorder(
                borderRadius: BorderRadius.circular(28),
                borderSide: BorderSide.none,
              ),
            ),
          ),
        ),
        // Dünne Fortschrittsleiste unter der AppBar, solange gesucht wird.
        bottom: PreferredSize(
          preferredSize: const Size.fromHeight(3),
          child: _loading
              ? const LinearProgressIndicator(minHeight: 3)
              : const SizedBox(height: 3),
        ),
      ),
      body: _query.isEmpty ? _buildStart(l) : _buildResults(l),
    );
  }

  // ------------------------------------------------------------ Startseite

  Widget _buildStart(L l) {
    return ListenableBuilder(
      listenable: Prefs.instance,
      builder: (context, _) {
        final recent = Prefs.instance.getList(_kRecentKey).take(_kMaxRecent).toList();
        return FutureBuilder<List<local.GenreCount>>(
          future: _genres,
          builder: (context, snap) {
            final genres = snap.data ?? const <local.GenreCount>[];
            final loaded = snap.connectionState == ConnectionState.done;
            return ListView(
              keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
              children: [
                if (recent.isNotEmpty) ...[
                  _Header(
                    l.searchRecent,
                    trailing: TextButton(
                      onPressed: () => Prefs.instance.setList(_kRecentKey, const []),
                      child: Text(l.searchClearAll),
                    ),
                  ),
                  for (final q in recent)
                    ListTile(
                      leading: const Icon(Icons.history),
                      title: Text(q, maxLines: 1, overflow: TextOverflow.ellipsis),
                      onTap: () => _setQueryAndSearch(q),
                      trailing: IconButton(
                        icon: const Icon(Icons.close),
                        tooltip: l.searchRemoveEntry,
                        onPressed: () => _removeRecent(q),
                      ),
                    ),
                ],
                if (genres.isNotEmpty) ...[
                  _Header(l.searchGenres),
                  _GenreGrid(genres: genres),
                ] else if (loaded)
                  // Ohne lokale Kopie gibt es keine Genres: freundlicher Hinweis.
                  Padding(
                    padding: const EdgeInsets.only(top: 48),
                    child: EmptyHint(
                      icon: Icons.search,
                      text: l.searchPrompt,
                      detail: l.searchPromptDetail,
                    ),
                  ),
                const SizedBox(height: 24),
              ],
            );
          },
        );
      },
    );
  }

  // ------------------------------------------------------------- Ergebnisse

  Widget _buildResults(L l) {
    final showTracks = _filter == _Filter.all || _filter == _Filter.tracks;
    final showAlbums = _filter == _Filter.all || _filter == _Filter.albums;
    final showArtists = _filter == _Filter.all || _filter == _Filter.artists;
    final empty = _tracks.isEmpty && _albums.isEmpty && _artists.isEmpty;

    // Bei "Alle" nur die ersten 6 Titel; "Alle anzeigen" wechselt zum Chip.
    final visibleTracks = _filter == _Filter.all ? _tracks.take(6).toList() : _tracks;

    return Column(
      children: [
        SizedBox(
          height: 52,
          child: ListView(
            scrollDirection: Axis.horizontal,
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
            children: [
              _chip(l.searchFilterAll, _Filter.all),
              _chip(l.tracks, _Filter.tracks),
              _chip(l.albums, _Filter.albums),
              _chip(l.artists, _Filter.artists),
            ],
          ),
        ),
        Expanded(
          child: empty
              ? (_loading
                    ? const SizedBox.shrink()
                    : EmptyHint(
                        icon: Icons.search_off,
                        text: l.searchNoResults(_query),
                        detail: AppState.instance.offline.value ? l.searchOfflineNote : null,
                      ))
              : ListView(
                  keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
                  children: [
                    if (showTracks && _tracks.isNotEmpty) ...[
                      _Header(
                        l.tracks,
                        trailing: _filter == _Filter.all && _tracks.length > 6
                            ? TextButton(
                                onPressed: () => setState(() => _filter = _Filter.tracks),
                                child: Text(l.searchShowAll),
                              )
                            : null,
                      ),
                      for (var i = 0; i < visibleTracks.length; i++)
                        TrackTile(
                          track: visibleTracks[i],
                          onTap: () {
                            _rememberQuery();
                            // Die ganze Trefferliste (nicht nur die sichtbaren
                            // 6) läuft ab dem angetippten Titel.
                            AppState.instance.player.playContext(_tracks, i, name: l.search);
                          },
                        ),
                    ],
                    if (showAlbums && _albums.isNotEmpty) ...[
                      _Header(l.albums),
                      for (final a in _albums)
                        _TapSpy(onTap: _rememberQuery, child: AlbumRow(album: a)),
                    ],
                    if (showArtists && _artists.isNotEmpty) ...[
                      _Header(l.artists),
                      for (final a in _artists)
                        _TapSpy(onTap: _rememberQuery, child: ArtistRow(artist: a)),
                    ],
                    const SizedBox(height: 24),
                  ],
                ),
        ),
      ],
    );
  }

  Widget _chip(String label, _Filter f) => Padding(
    padding: const EdgeInsets.symmetric(horizontal: 4),
    child: ChoiceChip(
      label: Text(label),
      selected: _filter == f,
      onSelected: (_) => setState(() => _filter = f),
    ),
  );
}

/// Überschrift eines Abschnitts, optional mit Knopf rechts.
class _Header extends StatelessWidget {
  const _Header(this.text, {this.trailing});

  final String text;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(16, 16, 8, 4),
    child: Row(
      children: [
        Expanded(
          child: Text(
            text,
            style: Theme.of(context).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.bold),
          ),
        ),
        if (trailing != null) trailing!,
      ],
    ),
  );
}

/// Beobachtet Antippen, ohne es zu verbrauchen.
///
/// WARUM: `AlbumRow`/`ArtistRow` navigieren selbst, ein `GestureDetector`
/// darum herum würde das Tippen nie bekommen. Ein `Listener` sieht die
/// Zeigerereignisse nur mit, ohne sie der Zeile wegzunehmen. Als "Tippen"
/// zählt Loslassen nahe am Aufsetzpunkt; Scrollen (große Bewegung) nicht.
class _TapSpy extends StatefulWidget {
  const _TapSpy({required this.onTap, required this.child});

  final VoidCallback onTap;
  final Widget child;

  @override
  State<_TapSpy> createState() => _TapSpyState();
}

class _TapSpyState extends State<_TapSpy> {
  Offset? _down;

  @override
  Widget build(BuildContext context) => Listener(
    onPointerDown: (e) => _down = e.position,
    onPointerUp: (e) {
      final d = _down;
      if (d != null && (e.position - d).distance < 18) widget.onTap();
      _down = null;
    },
    onPointerCancel: (_) => _down = null,
    child: widget.child,
  );
}

/// Genre-Kacheln: zwei Spalten, je 72 hoch.
class _GenreGrid extends StatelessWidget {
  const _GenreGrid({required this.genres});

  final List<local.GenreCount> genres;

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    // Kein GridView in der ListView (shrinkWrap): höchstens 30 Kacheln, daher
    // einfache Reihen aus zwei Expanded.
    final rows = (genres.length + 1) ~/ 2;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Column(
        children: [
          for (var r = 0; r < rows; r++)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Row(
                children: [
                  for (var c = 0; c < 2; c++) ...[
                    if (c == 1) const SizedBox(width: 8),
                    Expanded(
                      child: r * 2 + c < genres.length
                          ? _tile(context, l, genres[r * 2 + c], r * 2 + c)
                          : const SizedBox(height: 72),
                    ),
                  ],
                ],
              ),
            ),
        ],
      ),
    );
  }

  Widget _tile(BuildContext context, L l, local.GenreCount g, int index) {
    return Material(
      color: _genreColors[index % _genreColors.length],
      borderRadius: BorderRadius.circular(12),
      clipBehavior: Clip.antiAlias,
      child: InkWell(
        onTap: () => Navigator.of(context).push(
          MaterialPageRoute(
            builder: (_) => TrackListScreen(
              title: g.name,
              load: () => local.localGenreTracks(genre: g.name),
            ),
          ),
        ),
        child: SizedBox(
          height: 72,
          child: Padding(
            padding: const EdgeInsets.all(12),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  g.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: const TextStyle(color: Colors.white, fontWeight: FontWeight.bold),
                ),
                Text(
                  l.songs(g.trackCount),
                  style: const TextStyle(color: Colors.white70, fontSize: 12),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}
