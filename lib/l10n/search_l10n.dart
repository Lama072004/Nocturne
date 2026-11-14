import '../l10n.dart';

/// Texte des Suche-Tabs (siehe lib/ui/screens/search_screen.dart).
extension SearchL10n on L {
  String get searchRecent => t('Zuletzt gesucht', 'Recent searches');
  String get searchClearAll => t('Alle löschen', 'Clear all');
  String get searchRemoveEntry => t('Eintrag entfernen', 'Remove entry');
  String get searchClearField => t('Eingabe löschen', 'Clear search');
  String get searchGenres => t('Genres', 'Genres');
  String get searchShowAll => t('Alle anzeigen', 'Show all');
  String get searchFilterAll => t('Alle', 'All');
  String get searchPrompt => t('Was möchtest du hören?', 'What do you want to listen to?');
  String get searchPromptDetail => t(
    'Tippe oben einen Titel, Album oder Künstler ein.',
    'Type a track, album or artist above.',
  );
  String searchNoResults(String q) => t('Keine Treffer für »$q«', 'No results for “$q”');
  String get searchOfflineNote => t(
    'Offline wird nur die lokale Bibliothek durchsucht.',
    'While offline, only the local library is searched.',
  );
}
