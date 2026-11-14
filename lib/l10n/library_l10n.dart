import '../l10n.dart';

/// Texte des Bibliothek-Tabs.
///
/// Alle Namen beginnen mit `lib`, damit sie nie mit Texten aus `l10n.dart`
/// oder anderen Erweiterungen zusammenstoßen (bei gleichem Namen gewinnt
/// stillschweigend die Klasse selbst, nicht die Erweiterung).
extension LibraryL10n on L {
  // Filter-Chips
  String get libChipDownloaded => t('Heruntergeladen', 'Downloaded');
  String get libChipSmart => t('Smart', 'Smart');
  String get libClearFilter => t('Filter zurücksetzen', 'Clear filter');

  // Kopfzeile
  String get libFilterTooltip => t('In der Liste suchen', 'Filter list');
  String get libFilterHint => t('Nach Namen filtern', 'Filter by name');
  String get libNewPlaylistTooltip => t('Neue Playlist', 'New playlist');
  String libPlaylistCreated(String name) =>
      t('Playlist „$name“ erstellt', 'Created playlist "$name"');

  // Sortierung und Ansicht
  String get libSortName => t('Name', 'Name');
  String get libSortArtist => t('Künstler', 'Artist');
  String get libSortYear => t('Jahr', 'Year');
  String get libSortAdded => t('Zuletzt hinzugefügt', 'Recently added');
  String get libViewList => t('Als Liste anzeigen', 'Show as list');
  String get libViewGrid => t('Als Raster anzeigen', 'Show as grid');

  // Abschnitte und Zeilen
  String get libServerPlaylists => t('Playlists', 'Playlists');
  String libSongCount(int n) => t('$n Titel', n == 1 ? '1 song' : '$n songs');
  String libAlbumCount(int n) => t('$n Alben', n == 1 ? '1 album' : '$n albums');

  // Laden und leere Zustände
  String get libLoading => t('Bibliothek wird geladen …', 'Loading library …');
  String get libNoAlbums => t('Keine Alben gefunden', 'No albums found');
  String get libNoAlbumsDetail => t(
    'Die lokale Kopie ist leer. Ziehe die Liste nach unten, sobald du online bist.',
    'The local copy is empty. Pull down to refresh once you are online.',
  );
  String get libOfflineEmptyDetail => t(
    'Die Bibliothek wurde noch nie geladen. Verbinde dich einmal mit dem Server, dann steht sie auch offline bereit.',
    'The library has never been loaded. Connect to the server once and it will be available offline too.',
  );
  String get libNoArtists => t('Keine Künstler gefunden', 'No artists found');
  String get libNoPlaylists => t('Noch keine Playlists', 'No playlists yet');
  String get libNoPlaylistsDetail => t(
    'Lege mit dem Plus oben rechts eine Playlist an.',
    'Create one with the plus button at the top right.',
  );
  String get libNoFavorites => t('Noch keine Favoriten', 'No favorites yet');
  String get libNoFavoritesDetail => t(
    'Tippe bei Titeln, Alben oder Künstlern auf das Herz.',
    'Tap the heart on tracks, albums or artists.',
  );
  String get libNoDownloads => t('Noch nichts heruntergeladen', 'Nothing downloaded yet');
  String get libNoDownloadsDetail => t(
    'Öffne ein Album oder eine Playlist und tippe auf das Download-Symbol. Heruntergeladene Titel laufen auch ohne Netz.',
    'Open an album or playlist and tap the download icon. Downloaded tracks also play without a connection.',
  );
  String get libNoMatches => t('Keine Treffer', 'No matches');
  String get libGoToDownloads => t('Zu den Downloads', 'Go to downloads');

  // Heruntergeladen
  String libStorageUsed(String size) => t('$size belegt', '$size used');
}
