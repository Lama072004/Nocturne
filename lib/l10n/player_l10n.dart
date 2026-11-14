import '../l10n.dart';

/// Texte für Mini-Player, Vollbild-Player und Warteschlange.
extension PlayerL10n on L {
  String get playingNow => t('Läuft gerade', 'Now playing');
  String get playingFrom => t('Wiedergabe von', 'Playing from');
  String get closePlayer => t('Player schließen', 'Close player');
  String get moreOptions => t('Mehr', 'More');
  String get soundMenu => t('Klang', 'Sound');
  String get trackActions =>
      t('Weitere Aktionen zum Titel', 'More actions for this track');
  String get play => t('Wiedergabe', 'Play');
  String get pause => t('Pause', 'Pause');
  String get previousTrack => t('Vorheriger Titel', 'Previous track');
  String get nextTrack => t('Nächster Titel', 'Next track');
  String get shuffleTip => t('Zufällige Reihenfolge', 'Shuffle');
  String get repeatTip => t('Wiederholen', 'Repeat');
  String get addFavorite => t('Zu Favoriten', 'Add to favorites');
  String get removeFavorite => t('Aus Favoriten entfernen', 'Remove from favorites');
  String get sleepUntilTrackEnd =>
      t('Timer läuft bis zum Ende des Titels', 'Timer runs until the end of the track');
  String sleepEndsAt(String time) =>
      t('Timer endet um $time Uhr', 'Timer ends at $time');
  String get queueHint => t(
    'Wische einen Titel nach rechts, um ihn hier einzureihen.',
    'Swipe a track to the right to add it here.',
  );
  String get queueEmpty => t('Nichts in der Warteschlange', 'Nothing in the queue');
  String get toLibrary => t('Zur Bibliothek', 'Go to library');
  String get endSource => t('Quelle beenden', 'End source');
  String get dragToReorder => t('Zum Verschieben ziehen', 'Drag to reorder');
}
