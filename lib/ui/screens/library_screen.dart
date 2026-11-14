import 'package:flutter/material.dart';

import '../../app_state.dart';
import '../../l10n.dart';
import '../../l10n/library_l10n.dart';
import '../../prefs.dart';
import '../../src/rust/api/library.dart' as rust;
import '../../src/rust/api/local.dart' as local;
import '../../src/rust/api/models.dart';
import '../../src/rust/api/smart.dart' as smart;
import '../actions.dart';
import '../widgets/album_widgets.dart';
import '../widgets/async_view.dart';
import '../widgets/cover.dart';
import '../widgets/profile_button.dart';
import '../widgets/track_tile.dart';
import 'downloads_screen.dart';
import 'smart_playlist_screens.dart';
import 'track_list_screen.dart';

/// Die Namen der Filter-Chips in der Reihenfolge, in der sie erscheinen.
/// Dieselben Namen stehen im Merkzettel (`library.filter`, `library.sort.<chip>`,
/// `library.view.<chip>`), deshalb dürfen sie nicht umbenannt werden.
const _kChips = ['playlists', 'albums', 'artists', 'downloaded', 'favorites', 'smart'];

/// Ergebnis eines einzelnen Ladeschritts: Daten ODER Fehler.
///
/// WARUM: Die Übersicht lädt mehrere Teile (Smart-Playlists, Playlists,
/// Alben). Schlägt einer fehl, sollen die anderen trotzdem erscheinen; der
/// kaputte Teil zeigt nur bei sich "Erneut versuchen".
class _Part<T> {
  const _Part.ok(this.data) : error = null;
  const _Part.fail(this.error) : data = null;

  final T? data;
  final Object? error;
}

/// Alle Teile der Übersicht (ohne Chip). Teile, die offline nicht gehen,
/// sind `null` und werden gar nicht erst gezeigt (kein Fehler).
class _Overview {
  const _Overview({this.smartLists, this.playlists, required this.albums});

  final _Part<List<smart.SmartPlaylist>>? smartLists;
  final _Part<List<Playlist>>? playlists;
  final _Part<List<Album>> albums;
}

/// Bibliothek-Tab.
///
/// Ohne aktiven Chip zeigt er eine Übersicht (Lieblingssongs, Smart-
/// Playlists, Playlists, Alben). Mit Chip zeigt er genau eine Art Inhalt mit
/// eigener Sortierung und Ansicht. Alben und Künstler kommen aus der lokalen
/// Kopie der Bibliothek: sofort da und auch offline nutzbar.
class LibraryScreen extends StatefulWidget {
  const LibraryScreen({super.key});

  @override
  State<LibraryScreen> createState() => _LibraryScreenState();
}

class _LibraryScreenState extends State<LibraryScreen> {
  final _app = AppState.instance;
  final _prefs = Prefs.instance;

  /// Aktiver Chip oder `null` (= Übersicht).
  String? _filter;

  /// Lokales Filterfeld (Lupe in der Kopfzeile).
  bool _searching = false;
  final _queryController = TextEditingController();
  String _query = '';

  /// Daten des aktuellen Chips. Wird bei jedem Wechsel/Neuladen ersetzt.
  late Future<Object?> _future;

  /// `true`, solange die lokale Kopie zum ersten Mal vom Server geholt wird.
  /// Dann zeigt die Ladeanzeige den passenden Text.
  bool _syncing = false;

  @override
  void initState() {
    super.initState();
    final saved = _prefs.get('library.filter');
    _filter = _kChips.contains(saved) ? saved : null;
    _future = _load();
    // Wechselt der Netzstatus, passen sich die Inhalte an (offline gibt es
    // z. B. keine Server-Playlists).
    _app.offline.addListener(_reload);
  }

  @override
  void dispose() {
    _app.offline.removeListener(_reload);
    _queryController.dispose();
    super.dispose();
  }

  // ---------------------------------------------------------------------
  // Merkzettel
  // ---------------------------------------------------------------------

  /// Sortierung des Chips; Voreinstellung 'name'.
  String _sortOf(String chip) => _prefs.get('library.sort.$chip') ?? 'name';

  bool get _isGrid => _prefs.get('library.view.albums') == 'grid';

  local.AlbumSort get _albumSort => switch (_sortOf('albums')) {
    'artist' => local.AlbumSort.artist,
    'year' => local.AlbumSort.yearNewest,
    'added' => local.AlbumSort.addedNewest,
    _ => local.AlbumSort.name,
  };

  // ---------------------------------------------------------------------
  // Laden
  // ---------------------------------------------------------------------

  void _reload() {
    if (!mounted) return;
    setState(() {
      _future = _load();
    });
  }

  void _setFilter(String? chip) {
    // Leer = keiner. `null` löscht den Eintrag im Merkzettel.
    _prefs.set('library.filter', chip);
    setState(() {
      _filter = chip;
      // Die Smart-Liste hat ihr eigenes Layout; ein Filterfeld würde dort
      // nichts tun.
      if (chip == 'smart') _closeSearch();
      _future = _load();
    });
  }

  void _closeSearch() {
    _searching = false;
    _queryController.clear();
    _query = '';
  }

  Future<Object?> _load() {
    switch (_filter) {
      case null:
        return _loadOverview();
      case 'albums':
        return _syncedLocal(() => local.localAlbums(sort: _albumSort));
      case 'artists':
        return _syncedLocal(local.localArtists);
      case 'playlists':
        return _app.offline.value ? Future.value(null) : rust.playlists();
      case 'favorites':
        return _app.offline.value ? Future.value(null) : rust.starred();
      default:
        // Heruntergeladen und Smart laden selbst bzw. hören auf `downloads`.
        return Future.value(null);
    }
  }

  /// Liest aus der lokalen Kopie. Ist sie leer und wir sind online, wird sie
  /// zuerst vom Server geholt. WARUM hier: Beim allerersten Öffnen soll der
  /// Nutzer nicht vor einer leeren Seite stehen, sondern sofort sehen, dass
  /// etwas passiert.
  Future<T> _syncedLocal<T>(Future<T> Function() fetch) async {
    final info = await smart.librarySyncInfo();
    if (info.trackCount == 0 && !_app.offline.value) {
      if (mounted) setState(() => _syncing = true);
      try {
        await smart.syncLibrary();
      } finally {
        if (mounted) setState(() => _syncing = false);
      }
    }
    return fetch();
  }

  Future<_Part<T>> _try<T>(Future<T> f) async {
    try {
      return _Part<T>.ok(await f);
    } catch (e) {
      return _Part<T>.fail(e);
    }
  }

  Future<_Overview> _loadOverview() async {
    final offline = _app.offline.value;
    // Alle Teile gleichzeitig starten, dann erst warten.
    final smartF = offline ? null : _try(smart.listSmartPlaylists());
    final playlistsF = offline ? null : _try(rust.playlists());
    final albumsF = _try(_syncedLocal(() => local.localAlbums(sort: local.AlbumSort.addedNewest)));
    return _Overview(smartLists: await smartF, playlists: await playlistsF, albums: await albumsF);
  }

  /// Nach unten ziehen: Alben/Künstler bekommen frische Daten vom Server
  /// (nur online), alles andere lädt seine Liste neu.
  Future<void> _refresh() async {
    final canSync = _filter == null || _filter == 'albums' || _filter == 'artists';
    if (canSync && !_app.offline.value) {
      try {
        await smart.syncLibrary();
      } catch (e) {
        if (mounted) showToast(context, '$e');
      }
    }
    if (!mounted) return;
    _reload();
    try {
      await _future;
    } catch (_) {
      // Der Fehler wird in der Liste selbst angezeigt.
    }
  }

  // ---------------------------------------------------------------------
  // Aktionen
  // ---------------------------------------------------------------------

  Future<void> _newPlaylist() async {
    final l = L.of(context);
    final name = await showDialog<String>(
      context: context,
      builder: (_) => const _NameDialog(),
    );
    if (name == null || name.isEmpty) return;
    try {
      await rust.createPlaylist(name: name, songIds: []);
      if (!mounted) return;
      showToast(context, l.libPlaylistCreated(name));
      // Nur dort neu laden, wo Playlists sichtbar sind.
      if (_filter == null || _filter == 'playlists') _reload();
    } catch (e) {
      if (mounted) showToast(context, '$e');
    }
  }

  void _push(Widget page) => Navigator.of(context).push(MaterialPageRoute(builder: (_) => page));

  void _openLiked(L l) => _push(
    TrackListScreen(title: l.likedSongs, load: () async => (await rust.starred()).tracks),
  );

  void _openSmart(smart.SmartPlaylist p) => _push(
    TrackListScreen(title: p.name, load: () => smart.evaluateSmartPlaylist(id: p.id)),
  );

  void _openPlaylist(Playlist p) => _push(
    TrackListScreen(
      title: p.name,
      pinPlaylistId: p.id,
      load: () async => (await rust.playlistDetail(id: p.id)).tracks,
    ),
  );

  // ---------------------------------------------------------------------
  // Hilfen für die Anzeige
  // ---------------------------------------------------------------------

  /// Trifft [name] das Filterfeld? Ohne Eingabe trifft alles.
  bool _matches(String name) => _query.isEmpty || name.toLowerCase().contains(_query.toLowerCase());

  String _chipLabel(L l, String chip) => switch (chip) {
    'playlists' => l.playlists,
    'albums' => l.albums,
    'artists' => l.artists,
    'downloaded' => l.libChipDownloaded,
    'favorites' => l.favorites,
    _ => l.libChipSmart,
  };

  /// Scrollbare Seite mit Ziehen-zum-Aktualisieren. `AlwaysScrollable`, damit
  /// man auch bei kurzer Liste ziehen kann.
  Widget _scroll(List<Widget> slivers) => RefreshIndicator(
    onRefresh: _refresh,
    child: CustomScrollView(
      physics: const AlwaysScrollableScrollPhysics(),
      slivers: [...slivers, const SliverToBoxAdapter(child: SizedBox(height: 16))],
    ),
  );

  /// Leerer Zustand, der trotzdem nach unten gezogen werden kann.
  Widget _emptyScroll(Widget hint) => RefreshIndicator(
    onRefresh: _refresh,
    child: CustomScrollView(
      physics: const AlwaysScrollableScrollPhysics(),
      slivers: [SliverFillRemaining(hasScrollBody: false, child: hint)],
    ),
  );

  Widget _offlineHint(L l) => EmptyHint(
    icon: Icons.cloud_off_outlined,
    text: l.offlineNotAvailable,
    detail: l.offlineBanner,
    action: FilledButton.icon(
      icon: const Icon(Icons.download_done),
      label: Text(l.libGoToDownloads),
      onPressed: () => _setFilter('downloaded'),
    ),
  );

  /// Kleine Fehlerzeile innerhalb der Übersicht.
  Widget _inlineError(Object error) => SliverToBoxAdapter(
    child: Padding(
      padding: const EdgeInsets.symmetric(horizontal: 16),
      child: Row(
        children: [
          Expanded(
            child: Text(
              '$error',
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: Theme.of(context).colorScheme.error,
              ),
            ),
          ),
          TextButton(onPressed: _reload, child: Text(L.of(context).retry)),
        ],
      ),
    ),
  );

  // ---------------------------------------------------------------------
  // Bauen
  // ---------------------------------------------------------------------

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return Scaffold(
      appBar: AppBar(
        leading: const ProfileButton(),
        leadingWidth: 60,
        title: Text(l.library, style: const TextStyle(fontWeight: FontWeight.bold)),
        actions: [
          if (_filter != 'smart')
            IconButton(
              tooltip: l.libFilterTooltip,
              icon: Icon(_searching ? Icons.search_off : Icons.search),
              onPressed: () => setState(() {
                if (_searching) {
                  _closeSearch();
                } else {
                  _searching = true;
                }
              }),
            ),
          ValueListenableBuilder<bool>(
            valueListenable: _app.offline,
            builder: (context, offline, _) => IconButton(
              tooltip: l.libNewPlaylistTooltip,
              icon: const Icon(Icons.add),
              // Eine Playlist anlegen geht nur auf dem Server.
              onPressed: offline ? null : _newPlaylist,
            ),
          ),
        ],
      ),
      body: Column(
        children: [
          if (_searching)
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 4, 16, 8),
              child: TextField(
                controller: _queryController,
                autofocus: true,
                onChanged: (v) => setState(() => _query = v.trim()),
                decoration: InputDecoration(
                  hintText: l.libFilterHint,
                  prefixIcon: const Icon(Icons.search),
                  isDense: true,
                  border: OutlineInputBorder(borderRadius: BorderRadius.circular(24)),
                ),
              ),
            ),
          _buildChips(l),
          _buildToolbar(l),
          Expanded(child: _buildContent(l)),
        ],
      ),
    );
  }

  /// Chip-Reihe. Ist ein Chip aktiv, steht ganz links ein rundes ×.
  Widget _buildChips(L l) {
    final theme = Theme.of(context);
    return SizedBox(
      height: 52,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        children: [
          if (_filter != null)
            Center(
              child: Padding(
                padding: const EdgeInsets.only(right: 4),
                child: Tooltip(
                  message: l.libClearFilter,
                  child: Material(
                    color: theme.colorScheme.surfaceContainerHighest,
                    shape: const CircleBorder(),
                    child: InkWell(
                      customBorder: const CircleBorder(),
                      onTap: () => _setFilter(null),
                      // 48 x 48 Berührungsfläche, auch wenn der Kreis kleiner wirkt.
                      child: const SizedBox(
                        width: 48,
                        height: 48,
                        child: Icon(Icons.close, size: 20),
                      ),
                    ),
                  ),
                ),
              ),
            ),
          for (final chip in _kChips)
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: 4),
              child: Center(
                child: FilterChip(
                  label: Text(_chipLabel(l, chip)),
                  selected: _filter == chip,
                  showCheckmark: false,
                  onSelected: (_) => _setFilter(_filter == chip ? null : chip),
                ),
              ),
            ),
        ],
      ),
    );
  }

  /// Zeile unter den Chips: links Sortierung, rechts Liste/Raster.
  /// Erscheint nur dort, wo sie etwas bewirkt.
  Widget _buildToolbar(L l) {
    final chip = _filter;
    if (chip != 'albums' && chip != 'playlists' && chip != 'artists') {
      return const SizedBox.shrink();
    }
    final options = chip == 'albums'
        ? {
            'name': l.libSortName,
            'artist': l.libSortArtist,
            'year': l.libSortYear,
            'added': l.libSortAdded,
          }
        : {'name': l.libSortName};
    final current = options.containsKey(_sortOf(chip!)) ? _sortOf(chip) : 'name';
    final theme = Theme.of(context);

    return Padding(
      padding: const EdgeInsets.only(left: 16, right: 4),
      child: Row(
        children: [
          PopupMenuButton<String>(
            tooltip: l.sortBy,
            initialValue: current,
            onSelected: (v) {
              _prefs.set('library.sort.$chip', v);
              // Alben werden nach der Sortierung neu aus der Datenbank gelesen.
              _reload();
            },
            itemBuilder: (_) => [
              for (final e in options.entries) PopupMenuItem(value: e.key, child: Text(e.value)),
            ],
            child: ConstrainedBox(
              constraints: const BoxConstraints(minHeight: 48),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.swap_vert, size: 20),
                  const SizedBox(width: 6),
                  Text(options[current]!, style: theme.textTheme.labelLarge),
                ],
              ),
            ),
          ),
          const Spacer(),
          // Raster/Liste ergibt nur bei Alben Sinn (Cover als Hauptsache).
          if (chip == 'albums')
            IconButton(
              tooltip: _isGrid ? l.libViewList : l.libViewGrid,
              icon: Icon(_isGrid ? Icons.view_list : Icons.grid_view),
              onPressed: () {
                _prefs.set('library.view.albums', _isGrid ? 'list' : 'grid');
                setState(() {});
              },
            ),
        ],
      ),
    );
  }

  Widget _buildContent(L l) {
    final filter = _filter;
    if (filter == 'smart') return const SmartPlaylistList();
    if (filter == 'downloaded') return _buildDownloaded(l);

    return ValueListenableBuilder<bool>(
      valueListenable: _app.offline,
      builder: (context, offline, _) {
        // Playlists und Favoriten liegen nur auf dem Server.
        if (offline && (filter == 'playlists' || filter == 'favorites')) {
          return _offlineHint(l);
        }
        return FutureBuilder<Object?>(
          future: _future,
          builder: (context, snap) {
            if (snap.hasError) {
              return Center(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Padding(
                      padding: const EdgeInsets.all(24),
                      child: Text('${snap.error}', textAlign: TextAlign.center),
                    ),
                    FilledButton(onPressed: _reload, child: Text(l.retry)),
                  ],
                ),
              );
            }
            if (snap.connectionState != ConnectionState.done) return _buildLoading(l);
            final data = snap.data;
            return switch (filter) {
              null => _buildOverview(l, data as _Overview, offline),
              'albums' => _buildAlbums(l, data as List<Album>, offline),
              'artists' => _buildArtists(l, data as List<Artist>, offline),
              'playlists' => _buildPlaylists(l, data as List<Playlist>),
              _ => _buildFavorites(l, data as Starred),
            };
          },
        );
      },
    );
  }

  Widget _buildLoading(L l) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const CircularProgressIndicator(),
        // Beim ersten Abgleich dauert es länger: dann sagen, worauf wir warten.
        if (_syncing) ...[
          const SizedBox(height: 16),
          Text(l.libLoading),
        ],
      ],
    ),
  );

  // ---------------------------------------------------------------------
  // Inhalte
  // ---------------------------------------------------------------------

  Widget _sectionHeader(String text) => SliverToBoxAdapter(
    child: Padding(
      padding: const EdgeInsets.fromLTRB(16, 20, 16, 8),
      child: Text(
        text,
        style: Theme.of(context).textTheme.titleLarge?.copyWith(fontWeight: FontWeight.bold),
      ),
    ),
  );

  /// Lange Liste mit fester Zeilenhöhe (flüssiges Scrollen auch bei
  /// tausenden Einträgen: Flutter muss die Höhe nicht messen).
  Widget _fixedList(int count, Widget Function(int) item, {double extent = kCollectionRowHeight}) =>
      SliverFixedExtentList(
        itemExtent: extent,
        delegate: SliverChildBuilderDelegate((_, i) => item(i), childCount: count),
      );

  Widget _likedTile(L l) => SliverToBoxAdapter(
    child: _CollectionTile(
      leading: _IconBox(
        icon: Icons.favorite,
        background: Theme.of(context).colorScheme.primary,
        foreground: Theme.of(context).colorScheme.onPrimary,
      ),
      title: l.likedSongs,
      onTap: () => _openLiked(l),
    ),
  );

  Widget _smartTile(smart.SmartPlaylist p) => _CollectionTile(
    leading: _IconBox(
      icon: Icons.auto_awesome,
      background: Theme.of(context).colorScheme.surfaceContainerHighest,
      foreground: Theme.of(context).colorScheme.primary,
    ),
    title: p.name,
    onTap: () => _openSmart(p),
  );

  Widget _playlistTile(L l, Playlist p) => _CollectionTile(
    leading: Cover(coverId: p.coverArtId, size: 48),
    title: p.name,
    subtitle: l.libSongCount(p.songCount ?? 0),
    onTap: () => _openPlaylist(p),
  );

  /// Übersicht ohne Chip: Lieblingssongs, Smart, Playlists, Alben.
  Widget _buildOverview(L l, _Overview o, bool offline) {
    final slivers = <Widget>[];

    if (!offline && _matches(l.likedSongs)) slivers.add(_likedTile(l));

    final smartPart = o.smartLists;
    if (smartPart != null) {
      final items = [for (final p in smartPart.data ?? <smart.SmartPlaylist>[]) if (_matches(p.name)) p];
      if (smartPart.error != null) {
        slivers.add(_sectionHeader(l.smartPlaylists));
        slivers.add(_inlineError(smartPart.error!));
      } else if (items.isNotEmpty) {
        slivers.add(_sectionHeader(l.smartPlaylists));
        slivers.add(_fixedList(items.length, (i) => _smartTile(items[i])));
      }
    }

    final plPart = o.playlists;
    if (plPart != null) {
      final items = [for (final p in plPart.data ?? <Playlist>[]) if (_matches(p.name)) p];
      if (plPart.error != null) {
        slivers.add(_sectionHeader(l.playlists));
        slivers.add(_inlineError(plPart.error!));
      } else if (items.isNotEmpty) {
        slivers.add(_sectionHeader(l.playlists));
        slivers.add(_fixedList(items.length, (i) => _playlistTile(l, items[i])));
      }
    }

    final albumPart = o.albums;
    final albums = [for (final a in albumPart.data ?? <Album>[]) if (_matches(a.name)) a];
    if (albumPart.error != null) {
      slivers.add(_sectionHeader(l.albums));
      slivers.add(_inlineError(albumPart.error!));
    } else if (albums.isNotEmpty) {
      slivers.add(_sectionHeader(l.albums));
      slivers.add(_fixedList(albums.length, (i) => AlbumRow(key: ValueKey(albums[i].id), album: albums[i])));
    }

    if (slivers.isEmpty) {
      return _emptyScroll(_noMatchOr(l, offline ? _offlineHint(l) : _emptyAlbumsHint(l, false)));
    }
    return _scroll(slivers);
  }

  /// Bei aktivem Filter ohne Treffer "Keine Treffer", sonst der Hinweis.
  Widget _noMatchOr(L l, Widget fallback) => _query.isNotEmpty
      ? EmptyHint(icon: Icons.search_off, text: l.libNoMatches)
      : fallback;

  Widget _emptyAlbumsHint(L l, bool offline) => EmptyHint(
    icon: Icons.album_outlined,
    text: l.libNoAlbums,
    detail: offline ? l.libOfflineEmptyDetail : l.libNoAlbumsDetail,
  );

  Widget _buildAlbums(L l, List<Album> all, bool offline) {
    final albums = [for (final a in all) if (_matches(a.name)) a];
    if (albums.isEmpty) {
      return _emptyScroll(
        _noMatchOr(
          l,
          offline
              ? EmptyHint(
                  icon: Icons.cloud_off_outlined,
                  text: l.libNoAlbums,
                  detail: l.libOfflineEmptyDetail,
                  action: FilledButton.icon(
                    icon: const Icon(Icons.download_done),
                    label: Text(l.libGoToDownloads),
                    onPressed: () => _setFilter('downloaded'),
                  ),
                )
              : _emptyAlbumsHint(l, false),
        ),
      );
    }
    if (!_isGrid) {
      return _scroll([
        _fixedList(albums.length, (i) => AlbumRow(key: ValueKey(albums[i].id), album: albums[i])),
      ]);
    }
    // Raster: zwei Spalten. Die Cover-Größe folgt der Breite, damit es auf
    // jedem Gerät ohne Rand-Rest aufgeht.
    final textHeight = MediaQuery.textScalerOf(context).scale(44);
    return _scroll([
      SliverPadding(
        padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
        sliver: SliverLayoutBuilder(
          builder: (context, constraints) {
            const gap = 12.0;
            final size = (constraints.crossAxisExtent - gap) / 2;
            return SliverGrid(
              gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
                crossAxisCount: 2,
                mainAxisSpacing: gap,
                crossAxisSpacing: gap,
                // Cover + Abstand + zwei Textzeilen (mit Schriftgröße-Faktor).
                mainAxisExtent: size + 6 + textHeight,
              ),
              delegate: SliverChildBuilderDelegate(
                (_, i) => AlbumCard(key: ValueKey(albums[i].id), album: albums[i], size: size),
                childCount: albums.length,
              ),
            );
          },
        ),
      ),
    ]);
  }

  Widget _buildArtists(L l, List<Artist> all, bool offline) {
    final artists = [for (final a in all) if (_matches(a.name)) a]
      ..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    if (artists.isEmpty) {
      return _emptyScroll(
        _noMatchOr(
          l,
          EmptyHint(
            icon: Icons.person_outline,
            text: l.libNoArtists,
            detail: offline ? l.libOfflineEmptyDetail : l.libNoAlbumsDetail,
          ),
        ),
      );
    }
    return _scroll([
      _fixedList(
        artists.length,
        (i) => ArtistRow(
          key: ValueKey(artists[i].id),
          artist: artists[i],
          subtitle: l.libAlbumCount(artists[i].albumCount ?? 0),
        ),
      ),
    ]);
  }

  Widget _buildPlaylists(L l, List<Playlist> all) {
    final items = [for (final p in all) if (_matches(p.name)) p]
      ..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase()));
    if (items.isEmpty) {
      return _emptyScroll(
        _noMatchOr(
          l,
          EmptyHint(
            icon: Icons.queue_music,
            text: l.libNoPlaylists,
            detail: l.libNoPlaylistsDetail,
            action: FilledButton.icon(
              icon: const Icon(Icons.add),
              label: Text(l.libNewPlaylistTooltip),
              onPressed: _newPlaylist,
            ),
          ),
        ),
      );
    }
    return _scroll([_fixedList(items.length, (i) => _playlistTile(l, items[i]))]);
  }

  Widget _buildFavorites(L l, Starred s) {
    final albums = [for (final a in s.albums) if (_matches(a.name)) a];
    final artists = [for (final a in s.artists) if (_matches(a.name)) a];
    final showLiked = _matches(l.likedSongs);
    if (!showLiked && albums.isEmpty && artists.isEmpty) {
      return _emptyScroll(EmptyHint(icon: Icons.search_off, text: l.libNoMatches));
    }
    final slivers = <Widget>[
      if (showLiked) _likedTile(l),
      if (albums.isNotEmpty) ...[
        _sectionHeader(l.albums),
        _fixedList(albums.length, (i) => AlbumRow(key: ValueKey(albums[i].id), album: albums[i])),
      ],
      if (artists.isNotEmpty) ...[
        _sectionHeader(l.artists),
        _fixedList(
          artists.length,
          (i) => ArtistRow(
            key: ValueKey(artists[i].id),
            artist: artists[i],
            subtitle: l.libAlbumCount(artists[i].albumCount ?? 0),
          ),
        ),
      ],
    ];
    return _scroll(slivers);
  }

  /// Heruntergeladene Titel. Hört auf `downloads`, damit neue Downloads und
  /// Löschungen sofort sichtbar sind (ohne Neuladen und ohne Timer).
  Widget _buildDownloaded(L l) {
    final downloads = _app.downloads;
    return ListenableBuilder(
      listenable: downloads,
      builder: (context, _) {
        final all = downloads.tracks;
        if (all.isEmpty) {
          return EmptyHint(
            icon: Icons.download_for_offline_outlined,
            text: l.libNoDownloads,
            detail: l.libNoDownloadsDetail,
          );
        }
        final tracks = [for (final t in all) if (_matches(t.title)) t];
        return CustomScrollView(
          slivers: [
            SliverToBoxAdapter(
              child: ListTile(
                leading: const Icon(Icons.sd_storage_outlined),
                title: Text(l.libStorageUsed(formatBytes(downloads.totalBytes))),
                trailing: TextButton(
                  onPressed: () => _push(const DownloadsScreen()),
                  child: Text(l.manageDownloads),
                ),
              ),
            ),
            if (tracks.isEmpty)
              SliverFillRemaining(
                hasScrollBody: false,
                child: EmptyHint(icon: Icons.search_off, text: l.libNoMatches),
              )
            else
              _fixedList(
                tracks.length,
                (i) => TrackTile(
                  key: ValueKey(tracks[i].id),
                  track: tracks[i],
                  // Spielt die (ggf. gefilterte) Liste ab diesem Titel weiter.
                  onTap: () => _app.player.playContext(tracks, i, name: l.downloads),
                ),
                extent: kTrackRowHeight,
              ),
            const SliverToBoxAdapter(child: SizedBox(height: 16)),
          ],
        );
      },
    );
  }
}

/// Quadratisches Symbol (48 x 48) für Zeilen ohne Cover, z. B. das Herz bei
/// "Lieblingssongs" in der Akzentfarbe.
class _IconBox extends StatelessWidget {
  const _IconBox({required this.icon, required this.background, required this.foreground});

  final IconData icon;
  final Color background;
  final Color foreground;

  @override
  Widget build(BuildContext context) => Container(
    width: 48,
    height: 48,
    decoration: BoxDecoration(color: background, borderRadius: BorderRadius.circular(6)),
    child: Icon(icon, color: foreground),
  );
}

/// Einfache Listenzeile mit fester Höhe: links [leading], Name, optional
/// zweite Zeile. Gleiche Maße wie `AlbumRow`, damit die Liste ruhig aussieht.
class _CollectionTile extends StatelessWidget {
  const _CollectionTile({
    required this.leading,
    required this.title,
    required this.onTap,
    this.subtitle,
  });

  final Widget leading;
  final String title;
  final String? subtitle;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return InkWell(
      onTap: onTap,
      child: SizedBox(
        height: kCollectionRowHeight,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 16),
          child: Row(
            children: [
              leading,
              const SizedBox(width: 12),
              Expanded(
                child: Column(
                  mainAxisAlignment: MainAxisAlignment.center,
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: theme.textTheme.bodyLarge,
                    ),
                    if (subtitle != null) ...[
                      const SizedBox(height: 2),
                      Text(
                        subtitle!,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.bodyMedium?.copyWith(color: theme.hintColor),
                      ),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// Dialog "Neue Playlist": fragt den Namen ab und gibt ihn zurück.
///
/// Eigene Klasse, damit der Textcontroller sauber mit dem Dialog entsorgt
/// wird (sonst Speicherleck beim Schließen).
class _NameDialog extends StatefulWidget {
  const _NameDialog();

  @override
  State<_NameDialog> createState() => _NameDialogState();
}

class _NameDialogState extends State<_NameDialog> {
  final _controller = TextEditingController();

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  void _submit() => Navigator.of(context).pop(_controller.text.trim());

  @override
  Widget build(BuildContext context) {
    final l = L.of(context);
    return AlertDialog(
      title: Text(l.newPlaylist),
      content: TextField(
        controller: _controller,
        autofocus: true,
        textInputAction: TextInputAction.done,
        onSubmitted: (_) => _submit(),
        decoration: InputDecoration(hintText: l.playlistName),
      ),
      actions: [
        TextButton(onPressed: () => Navigator.of(context).pop(), child: Text(l.cancel)),
        FilledButton(onPressed: _submit, child: Text(l.create)),
      ],
    );
  }
}
