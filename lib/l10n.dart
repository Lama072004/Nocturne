import 'package:flutter/widgets.dart';

import 'src/rust/api/settings.dart';
import 'src/rust/api/smart.dart';

/// Alle sichtbaren Texte der App auf Deutsch und Englisch.
///
/// WARUM eine eigene kleine Klasse statt `flutter gen-l10n` mit .arb-Dateien?
/// Für zwei Sprachen ist das hier am einfachsten zu lesen und zu ändern:
/// beide Übersetzungen stehen direkt nebeneinander, und der Compiler meldet
/// fehlende Texte sofort. Kommen später mehr Sprachen dazu, lässt sich das
/// mechanisch auf .arb-Dateien umstellen, weil kein Widget Texte fest enthält.
class L {
  L(this.de);

  /// `true` = Deutsch, sonst Englisch.
  final bool de;

  static L of(BuildContext context) =>
      L(Localizations.localeOf(context).languageCode == 'de');

  String _(String german, String english) => de ? german : english;

  /// Öffentliche Fassung von `_`: Damit können einzelne Bildschirme ihre
  /// eigenen Texte in einer eigenen Datei (als `extension on L`) pflegen,
  /// ohne dass alle an dieser einen Datei ändern müssen.
  String t(String german, String english) => _(german, english);

  // Navigation
  String get home => _('Home', 'Home');
  String get search => _('Suche', 'Search');
  String get library => _('Bibliothek', 'Library');
  String get downloads => _('Downloads', 'Downloads');
  String get stats => _('Statistik', 'Stats');
  String get settings => _('Einstellungen', 'Settings');

  // Anmeldung
  String get loginTitle => _('Mit Server verbinden', 'Connect to server');
  String get serverType => _('Servertyp', 'Server type');
  String get serverUrl => _('Server-Adresse', 'Server address');
  String get username => _('Benutzername', 'Username');
  String get password => _('Passwort', 'Password');
  String get connect => _('Verbinden', 'Connect');
  String get logout => _('Abmelden', 'Log out');
  String get notYetSupported => _('folgt', 'coming soon');
  String get fillAllFields =>
      _('Bitte alle Felder ausfüllen', 'Please fill in all fields');

  // Home / Suche
  String get recentlyPlayed => _('Zuletzt gehört', 'Recently played');
  String get newlyAdded => _('Neu hinzugefügt', 'Newly added');
  String get searchHint =>
      _('Was möchtest du hören?', 'What do you want to listen to?');
  String get artists => _('Künstler', 'Artists');
  String get albums => _('Alben', 'Albums');
  String get tracks => _('Titel', 'Tracks');
  String get noResults => _('Nichts gefunden', 'No results');
  String get retry => _('Erneut versuchen', 'Retry');
  String get comingSoon => _(
    'Dieser Bereich wird in einem späteren Schritt gebaut.',
    'This area will be built in a later step.',
  );

  // Player / Warteschlange
  String get queue => _('Warteschlange', 'Queue');
  String get nowPlaying => _('Aktueller Titel', 'Now playing');
  String get nextInQueue =>
      _('Als Nächstes in der Warteschlange', 'Next in queue');
  String nextFrom(String? name) => name == null
      ? _('Als Nächstes', 'Next up')
      : _('Als Nächstes von: $name', 'Next from: $name');
  String get clearQueue => _('Leeren', 'Clear');
  String get addedToQueue =>
      _('Zur Warteschlange hinzugefügt', 'Added to queue');

  /// Kurzform für die Pille hinter dem Titelnamen ("Song · in Warteschlange").
  String get addedToQueueShort => _('in Warteschlange', 'queued');
  String tracksAddedToQueue(int n) => _(
    '$n Titel in Warteschlange',
    n == 1 ? '1 track queued' : '$n tracks queued',
  );
  String get removedFromQueue =>
      _('Aus Warteschlange entfernt', 'Removed from queue');
  String get actionNotReady =>
      _('Diese Aktion ist noch nicht gebaut', 'This action is not built yet');
  String get play => _('Abspielen', 'Play');
  String songs(int n) => _('$n Titel', n == 1 ? '1 track' : '$n tracks');

  // Einstellungen
  String get appearance => _('Darstellung', 'Appearance');
  String get theme => _('Design', 'Theme');
  String get accentColor => _('Akzentfarbe', 'Accent color');
  String get systemDark =>
      _('Dunkel-Variante bei „System“', 'Dark variant for “System”');
  String get quality => _('Klangqualität', 'Audio quality');
  String get qualityWifi => _('Streaming im WLAN', 'Streaming on Wi-Fi');
  String get qualityMobile =>
      _('Streaming mit mobilen Daten', 'Streaming on mobile data');
  String get qualityDownload => _('Downloads', 'Downloads');
  String get swipes => _('Wischgesten', 'Swipe gestures');
  String get swipeTrackRows => _('Titel-Zeilen', 'Track rows');
  String get swipeCollectionRows =>
      _('Album- und Playlist-Zeilen', 'Album and playlist rows');
  String get swipeQueueRows => _('Warteschlange', 'Queue');
  String get swipeMiniPlayer =>
      _('Mini-Player und Cover', 'Mini player and cover');
  String get rightShort => _('Kurz nach rechts', 'Short right');
  String get rightLong => _('Lang nach rechts', 'Long right');
  String get leftShort => _('Kurz nach links', 'Short left');
  String get leftLong => _('Lang nach links', 'Long left');
  String get swipeLeft => _('Nach links', 'Left');
  String get swipeRight => _('Nach rechts', 'Right');
  String get swipeUp => _('Nach oben', 'Up');
  String get offline => _('Offline', 'Offline');
  String get offlineOnMobile => _(
    'In mobilen Daten nur Downloads anzeigen',
    'Show only downloads on mobile data',
  );
  String get downloadWifiOnly =>
      _('Downloads nur im WLAN', 'Download on Wi-Fi only');

  // Bibliothek
  String get playlists => _('Playlists', 'Playlists');
  String get favorites => _('Favoriten', 'Favorites');
  String get newPlaylist => _('Neue Playlist', 'New playlist');
  String get playlistName => _('Name der Playlist', 'Playlist name');
  String get create => _('Erstellen', 'Create');
  String get cancel => _('Abbrechen', 'Cancel');
  String get addToPlaylistTitle =>
      _('Zur Playlist hinzufügen', 'Add to playlist');
  String addedToPlaylist(String name) =>
      _('Zu „$name“ hinzugefügt', 'Added to “$name”');
  String get addedToFavorites =>
      _('Zu Favoriten hinzugefügt', 'Added to favorites');
  String get removedFromFavorites =>
      _('Aus Favoriten entfernt', 'Removed from favorites');
  String get empty => _('Noch nichts vorhanden', 'Nothing here yet');
  String albumsCount(int n) =>
      _(n == 1 ? '1 Album' : '$n Alben', n == 1 ? '1 album' : '$n albums');
  String get shufflePlay => _('Zufällig', 'Shuffle');

  // Downloads / Offline
  String get downloadStarted => _('Download gestartet', 'Download started');
  String get alreadyDownloaded =>
      _('Bereits heruntergeladen', 'Already downloaded');
  String get downloadLimitReached => _(
    'Speicherlimit für Downloads erreicht',
    'Download storage limit reached',
  );
  String get downloadRemoved => _('Download entfernt', 'Download removed');
  String get removeAllDownloads =>
      _('Alle Downloads löschen', 'Delete all downloads');
  String get removeAllDownloadsConfirm => _(
    'Wirklich alle heruntergeladenen Titel löschen?',
    'Really delete all downloaded tracks?',
  );
  String get delete => _('Löschen', 'Delete');
  String get noDownloads => _('Noch keine Downloads', 'No downloads yet');
  String get noDownloadsHint => _(
    'Wische auf einem Titel oder Album, um es herunterzuladen.',
    'Swipe a track or album to download it.',
  );
  String storageUsed(String size, int n) =>
      _('$size · $n Titel', n == 1 ? '$size · 1 track' : '$size · $n tracks');
  String get waiting => _('Wartet', 'Waiting');
  String get failed => _('Fehlgeschlagen', 'Failed');
  String get offlineBanner => _(
    'Offline – es werden nur Downloads angezeigt',
    'Offline – showing downloads only',
  );
  String get offlineNotAvailable =>
      _('Offline nicht verfügbar', 'Not available offline');
  String get autoOffline => _(
    'Ohne Netz automatisch offline',
    'Go offline automatically without network',
  );
  String get downloadLimit =>
      _('Speicherlimit für Downloads', 'Download storage limit');
  String get streamCache => _('Streaming-Cache', 'Streaming cache');
  String get unlimited => _('Unbegrenzt', 'Unlimited');
  String get off => _('Aus', 'Off');
  String get sdCard =>
      _('Downloads auf SD-Karte speichern', 'Store downloads on SD card');
  String get sdCardHint => _(
    'Wirkt nach einem Neustart der App',
    'Takes effect after restarting the app',
  );

  // Statistik
  String get last7Days => _('7 Tage', '7 days');
  String get last30Days => _('30 Tage', '30 days');
  String get lastYear => _('1 Jahr', '1 year');
  String get allTime => _('Gesamt', 'All time');
  String get plays => _('Wiedergaben', 'Plays');
  String get listeningTime => _('Hörzeit', 'Listening time');
  String get topTracks => _('Top-Titel', 'Top tracks');
  String get topArtists => _('Top-Künstler', 'Top artists');
  String get topAlbums => _('Top-Alben', 'Top albums');
  String get activity => _('Aktivität', 'Activity');
  String get byHour => _('Nach Tageszeit', 'By time of day');
  String get history => _('Verlauf', 'History');
  String get noStats => _(
    'Noch keine Daten. Die Statistik füllt sich, sobald du Musik hörst.',
    'No data yet. Stats fill up as you listen.',
  );
  String playsCount(int n) => _('$n×', '$n×');

  // Player-Extras
  String get lyrics => _('Songtext', 'Lyrics');
  String get noLyrics => _('Kein Songtext vorhanden', 'No lyrics available');
  String get sleepTimer => _('Sleep-Timer', 'Sleep timer');
  String get sleepOff => _('Timer aus', 'Turn off timer');
  String get endOfTrack => _('Ende des Titels', 'End of track');
  String minutes(int n) => _('$n Minuten', '$n minutes');
  String get sleepSet => _('Sleep-Timer gestellt', 'Sleep timer set');

  // Klang
  String get sound => _('Klang', 'Sound');
  String get equalizer => _('Equalizer', 'Equalizer');
  String get effectsBatteryHint => _(
    'Eingeschaltete Effekte brauchen etwas mehr Akku.',
    'Enabled effects use a bit more battery.',
  );
  String deviceProfile(String name) =>
      _('Eigenes Profil für „$name“', 'Own profile for “$name”');
  String get deviceProfileOn => _(
    'Änderungen gelten nur für dieses Gerät',
    'Changes apply to this device only',
  );
  String get deviceProfileOff =>
      _('Es gilt das Standard-Profil', 'The default profile is used');

  /// Anzeigename eines Ausgabegeräts. Android liefert für Bluetooth und
  /// USB den echten Gerätenamen; für Lautsprecher und Kabel nur einen
  /// Schlüssel, den wir hier übersetzen.
  String outputDeviceName(String key, String fallback) => switch (key) {
    'speaker' => _('Lautsprecher', 'Speaker'),
    'wired' => _('Kabel-Kopfhörer', 'Wired headphones'),
    _ => fallback,
  };

  String get eqGraphic => _('10 Bänder', '10 bands');
  String get eqParametric => _('Parametrisch', 'Parametric');
  String get autoEq => _('AutoEQ-Kopfhörerprofil', 'AutoEQ headphone profile');
  String get autoEqHint => _(
    'Kopfhörer suchen oder Profil-Text einfügen',
    'Search headphones or paste profile text',
  );
  String get autoEqUseText =>
      _('Eingefügtes Profil verwenden', 'Use pasted profile');
  String get reset => _('Zurücksetzen', 'Reset');
  String get preamp => _('Vorverstärkung', 'Preamp');
  String get addBand => _('Band hinzufügen', 'Add band');
  String get frequency => _('Frequenz', 'Frequency');
  String get gain => _('Verstärkung', 'Gain');
  String get qFactor => _('Güte (Q)', 'Q factor');
  String get filterPeaking => _('Glocke', 'Peaking');
  String get filterLowShelf => _('Bass-Shelf', 'Low shelf');
  String get filterHighShelf => _('Höhen-Shelf', 'High shelf');
  String get compressor => _('Kompressor', 'Compressor');
  String get compressorHint => _(
    'Gleicht Unterschiede zwischen leisen und lauten Stellen aus.',
    'Evens out differences between quiet and loud passages.',
  );
  String get threshold => _('Schwelle', 'Threshold');
  String get ratio => _('Verhältnis', 'Ratio');
  String get attack => _('Ansprechzeit', 'Attack');
  String get release => _('Rücklaufzeit', 'Release');
  String get makeup => _('Ausgleich', 'Makeup gain');
  String get replayGain => _('ReplayGain', 'ReplayGain');
  String get replayGainHint => _(
    'Gleiche Lautstärke über alle Titel und Alben hinweg.',
    'Consistent volume across all tracks and albums.',
  );
  String get rgTrack => _('Titel', 'Track');
  String get rgAlbum => _('Album', 'Album');
  String get rgFallback => _('Ohne Tags', 'Untagged');
  String get clippingProtection =>
      _('Schutz vor Übersteuerung', 'Clipping protection');

  // Smart-Playlists
  String get smart => _('Smart', 'Smart');
  String get smartPlaylist => _('Smart-Playlist', 'Smart playlist');
  String get newSmartPlaylist => _('Neue Smart-Playlist', 'New smart playlist');
  String get smartEmpty => _('Leer beginnen', 'Start empty');
  String get edit => _('Bearbeiten', 'Edit');
  String get save => _('Speichern', 'Save');
  String get addRule => _('Regel', 'Rule');
  String get addGroup => _('Gruppe', 'Group');
  String get matchAll => _('Alle müssen zutreffen', 'All must match');
  String get matchAny => _('Eine reicht', 'Any may match');
  String smartMatches(int n) =>
      _('$n Titel passen zu den Regeln', '$n tracks match the rules');
  String get sortBy => _('Sortierung', 'Sort by');
  String get limitCount => _('Höchstens Titel', 'Max. tracks');
  String get limitMinutes => _('Höchstens Minuten', 'Max. minutes');
  String get mirrorToServer =>
      _('Auch auf dem Server speichern', 'Also save on the server');
  String get mirrorToServerHint => _(
    'Als normale Playlist, damit andere Geräte sie sehen. Wird beim Öffnen aktualisiert.',
    'As a regular playlist so other devices see it. Updated when opened.',
  );
  String get autoDownload =>
      _('Automatisch herunterladen', 'Download automatically');
  String get autoDownloadHint => _(
    'Braucht den Schalter „Smart-Playlists“ unter Einstellungen › Offline.',
    'Needs the “Smart playlists” switch under Settings › Offline.',
  );
  String get libraryNeverSynced =>
      _('Titel-Liste noch nicht geladen', 'Track list not loaded yet');
  String libraryTracks(int n) =>
      _('$n Titel in der lokalen Liste', '$n tracks in the local list');
  String get librarySyncHint => _(
    'Tippen, um die Titel-Liste vom Server zu aktualisieren',
    'Tap to refresh the track list from the server',
  );
  String librarySynced(int n) => _('$n Titel geladen', '$n tracks loaded');
  String get tplForgottenFavorites =>
      _('Vergessene Favoriten', 'Forgotten favorites');
  String get tplNewlyAdded => _('Neu hinzugefügt', 'Newly added');
  String get tplMostPlayed => _('Meistgehört', 'Most played');
  String get tplNeverPlayed => _('Noch nie gehört', 'Never played');
  String get tplTopRated => _('Am besten bewertet', 'Top rated');
  String get keepOffline =>
      _('Offline aktuell halten', 'Keep available offline');
  String get keepOfflineOn => _(
    'Wird offline aktuell gehalten (Schalter „Playlists“ unter Einstellungen › Offline)',
    'Kept available offline (switch “Playlists” under Settings › Offline)',
  );
  String get autoSync =>
      _('Automatisch herunterladen', 'Download automatically');
  String get autoSyncFavorites => _('Favoriten', 'Favorites');
  String get autoSyncPlaylists =>
      _('Vorgemerkte Playlists', 'Pinned playlists');
  String get autoSyncSmart => _('Smart-Playlists', 'Smart playlists');

  String ruleFieldName(RuleField f) => switch (f) {
    RuleField.title => _('Titel', 'Title'),
    RuleField.artist => _('Künstler', 'Artist'),
    RuleField.album => _('Album', 'Album'),
    RuleField.genre => _('Genre', 'Genre'),
    RuleField.year => _('Jahr', 'Year'),
    RuleField.duration => _('Länge (Sek.)', 'Length (sec)'),
    RuleField.starred => _('Favorit', 'Favorite'),
    RuleField.rating => _('Bewertung', 'Rating'),
    RuleField.playCount => _('Wiedergaben', 'Plays'),
    RuleField.daysSincePlayed => _('Tage seit gehört', 'Days since played'),
    RuleField.daysSinceAdded => _('Tage seit hinzugefügt', 'Days since added'),
    RuleField.downloaded => _('Heruntergeladen', 'Downloaded'),
  };

  String ruleOpName(RuleOp o) => switch (o) {
    RuleOp.is_ => _('ist', 'is'),
    RuleOp.isNot => _('ist nicht', 'is not'),
    RuleOp.contains => _('enthält', 'contains'),
    RuleOp.notContains => _('enthält nicht', 'does not contain'),
    RuleOp.greater => _('größer als', 'greater than'),
    RuleOp.less => _('kleiner als', 'less than'),
    RuleOp.isTrue => _('ja', 'yes'),
    RuleOp.isFalse => _('nein', 'no'),
  };

  String smartSortName(SmartSort s) => switch (s) {
    SmartSort.random => _('Zufällig', 'Random'),
    SmartSort.title => _('Titel', 'Title'),
    SmartSort.artist => _('Künstler', 'Artist'),
    SmartSort.album => _('Album', 'Album'),
    SmartSort.yearNewest => _('Neuestes Jahr', 'Newest year'),
    SmartSort.addedNewest => _('Zuletzt hinzugefügt', 'Recently added'),
    SmartSort.mostPlayed => _('Meistgehört', 'Most played'),
    SmartSort.leastPlayed => _('Am wenigsten gehört', 'Least played'),
    SmartSort.playedLongestAgo => _(
      'Am längsten nicht gehört',
      'Played longest ago',
    ),
    SmartSort.playedRecently => _('Zuletzt gehört', 'Recently played'),
    SmartSort.bestRated => _('Beste Bewertung', 'Best rated'),
  };

  // Kontextmenü
  String get moreOptions => _('Weitere Aktionen', 'More options');
  String get addFavorite => _('Zu Favoriten hinzufügen', 'Add to favorites');
  String get removeFavorite =>
      _('Aus Favoriten entfernen', 'Remove from favorites');
  String get removeFromQueueAction =>
      _('Aus Warteschlange entfernen', 'Remove from queue');
  String get undo => _('Rückgängig', 'Undo');

  // Profil
  String get profile => _('Profil', 'Profile');
  String get yourStats => _('Deine Statistik', 'Your stats');
  String get last4Weeks => _('Letzte 4 Wochen', 'Last 4 weeks');
  String get manageDownloads => _('Downloads verwalten', 'Manage downloads');
  String get online => _('Online', 'Online');
  String get notLoggedIn => _('Nicht angemeldet', 'Not signed in');
  String get logoutConfirm => _(
    'Abmelden? Heruntergeladene Titel bleiben auf dem Gerät.',
    'Log out? Downloaded tracks stay on the device.',
  );

  // Home
  String get goodMorning => _('Guten Morgen', 'Good morning');
  String get goodAfternoon => _('Guten Tag', 'Good afternoon');
  String get goodEvening => _('Guten Abend', 'Good evening');
  String get yourTopTracks => _('Deine Top-Titel', 'Your top tracks');
  String get smartPlaylists => _('Smart-Playlists', 'Smart playlists');
  String get shuffleAll => _('Alles zufällig', 'Shuffle all');
  String get likedSongs => _('Lieblingssongs', 'Liked songs');

  String themeName(ThemeChoice t) => switch (t) {
    ThemeChoice.dark => _('Dunkel', 'Dark'),
    ThemeChoice.black => _('Echtes Schwarz', 'True black'),
    ThemeChoice.light => _('Hell', 'Light'),
    ThemeChoice.system => _('System', 'System'),
  };

  String darkVariantName(DarkVariant v) => switch (v) {
    DarkVariant.dark => _('Dunkel', 'Dark'),
    DarkVariant.black => _('Echtes Schwarz', 'True black'),
  };

  String formatName(AudioFormat f) => switch (f) {
    AudioFormat.original => _('Original', 'Original'),
    AudioFormat.opus => 'Opus',
    AudioFormat.aac => 'AAC',
    AudioFormat.mp3 => 'MP3',
  };

  String swipeActionName(SwipeAction a) => switch (a) {
    SwipeAction.none => _('Keine', 'None'),
    SwipeAction.addToQueue => _('Zur Warteschlange', 'Add to queue'),
    SwipeAction.toggleFavorite => _('Favorit', 'Favorite'),
    SwipeAction.download => _('Herunterladen', 'Download'),
    SwipeAction.addToPlaylist => _('Zur Playlist', 'Add to playlist'),
    SwipeAction.goToAlbum => _('Zum Album', 'Go to album'),
    SwipeAction.goToArtist => _('Zum Künstler', 'Go to artist'),
    SwipeAction.removeFromQueue => _('Entfernen', 'Remove'),
  };

  String playerSwipeName(PlayerSwipeAction a) => switch (a) {
    PlayerSwipeAction.none => _('Keine', 'None'),
    PlayerSwipeAction.nextTrack => _('Nächster Titel', 'Next track'),
    PlayerSwipeAction.previousTrack => _('Vorheriger Titel', 'Previous track'),
    PlayerSwipeAction.openPlayer => _('Player öffnen', 'Open player'),
    PlayerSwipeAction.togglePlay => _('Play/Pause', 'Play/pause'),
  };
}
