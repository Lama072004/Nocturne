import '../l10n.dart';

/// Texte für Album-, Künstler-, Titellisten-Seite, Statistik und Anmeldung.
///
/// Als Erweiterung von [L] in eigener Datei, damit parallel an anderen
/// Bereichen gearbeitet werden kann, ohne `l10n.dart` anzufassen.
extension DetailL10n on L {
  // --- Album / Künstler / Titellisten ---

  /// Zwischenüberschrift bei Alben mit mehreren CDs.
  String discLabel(int n) => t('CD $n', 'Disc $n');

  /// Gesamtlänge, z. B. "1 Std. 12 Min." (Sekunden werden gerundet).
  String totalLength(int secs) {
    final h = secs ~/ 3600;
    final m = (secs % 3600) ~/ 60;
    if (h > 0) return t('$h Std. $m Min.', '$h hr $m min');
    return t('${m < 1 ? 1 : m} Min.', '${m < 1 ? 1 : m} min');
  }

  String get downloadAll => t('Alle herunterladen', 'Download all');
  String get allDownloaded => t('Alles heruntergeladen', 'All downloaded');
  String get albumUnavailable => t(
    'Dieses Album ist offline nicht verfügbar.',
    'This album is not available offline.',
  );
  String get artistUnnamed => t('Künstler', 'Artist');
  String get refreshHint => t('Zum Aktualisieren nach unten ziehen', 'Pull down to refresh');

  // --- Statistik ---

  String get period4Weeks => t('4 Wochen', '4 weeks');
  String get period6Months => t('6 Monate', '6 months');
  String get period1Year => t('1 Jahr', '1 year');
  String get showAll => t('Alle anzeigen', 'Show all');
  String get showLess => t('Weniger', 'Show less');
  String get statsEmpty => t(
    'Hör ein paar Songs – deine Statistik erscheint hier.',
    'Listen to a few songs – your stats will show up here.',
  );

  // --- Anmeldung ---

  String get hintSubsonic => t(
    'Für Navidrome, Gonic, Airsonic, Ampache und andere Subsonic-Server.',
    'For Navidrome, Gonic, Airsonic, Ampache and other Subsonic servers.',
  );
  String get hintJellyfin => t(
    'Mit Benutzername und Passwort deines Jellyfin-Servers anmelden.',
    'Sign in with your Jellyfin server username and password.',
  );
  String get hintEmby => t(
    'Mit Benutzername und Passwort deines Emby-Servers anmelden.',
    'Sign in with your Emby server username and password.',
  );
  String get hintPlex => t(
    'Mit Plex-Konto anmelden. Benutzername leer lassen, um stattdessen ein Plex-Token ins Passwortfeld einzugeben.',
    'Sign in with your Plex account. Leave the username empty to enter a Plex token in the password field instead.',
  );
  String get hintUntested => t(
    'Noch nicht mit einem echten Server getestet.',
    'Not yet tested against a real server.',
  );
  String get serverUrlHint => t(
    'z. B. musik.example.org:4533 (ohne http:// erlaubt)',
    'e.g. music.example.org:4533 (http:// is optional)',
  );
  String get showPassword => t('Passwort anzeigen', 'Show password');
  String get hidePassword => t('Passwort verbergen', 'Hide password');
  String get connecting => t('Verbinde …', 'Connecting …');
  String get tryDemo => t('Demo ausprobieren', 'Try the demo');
  String get tryDemoHint => t(
    'Füllt die Felder mit dem öffentlichen Navidrome-Demo-Server.',
    'Fills in the public Navidrome demo server.',
  );
  String get loginFailedGeneric => t(
    'Anmeldung fehlgeschlagen. Prüfe Adresse, Zugangsdaten und Verbindung.',
    'Sign-in failed. Check the address, credentials and connection.',
  );
  String get fillRequiredFields => t(
    'Bitte Adresse, Benutzername und Passwort ausfüllen.',
    'Please fill in address, username and password.',
  );
  String get fillUrlAndSecret => t(
    'Bitte Adresse und Passwort (oder Plex-Token) ausfüllen.',
    'Please fill in the address and password (or Plex token).',
  );
}
