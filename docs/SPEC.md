# Spezifikation – Musicplayer

Stand: 2026-11-11. Ergebnis der Anforderungsfragen. Ziel: Ersatz für Substreamer, Bedienung wie Spotify.
Substreamer (`D:\Mathias Lampert\Projects\substreamer`) dient nur als Referenz zum Nachlesen.

## 1. Architektur

| Schicht | Technik | Aufgabe |
|---|---|---|
| UI | Flutter (Dart) | Komplette Oberfläche, Gesten, Animationen |
| Kern | Rust via `flutter_rust_bridge` | Server-Adapter, lokale DB, Bibliothek-Cache, Warteschlange, Smart-Playlists, Downloads, Scrobble-Queue, Einstellungen |
| Audio-DSP | Rust (JNI) im Media3-Audio-Pfad | Parametrischer EQ, 10-Band-EQ, Kompressor, ReplayGain (Preamp + Clipping-Schutz), Crossfade |
| Android-Native | Kotlin, Media3 (`MediaLibraryService`, ExoPlayer) | Wiedergabe, MediaSession, Benachrichtigung, Android Auto, Widgets, WorkManager-Downloads |

- **Plattform:** jetzt nur Android (Ziel Android 17), Architektur so, dass Windows/Linux später ohne Umbau möglich sind (plattformneutraler Rust-Kern, Audio-Backend austauschbar).
- **Akku:** Hardware-Audio-Offload, wenn alle Effekte aus sind; keine Polling-Timer; Downloads/Syncs über WorkManager mit Constraints; Bilder gecacht und skaliert; echtes-Schwarz-Theme.

## 2. Server

Adapter-Schnittstelle im Rust-Kern, je Server ein Adapter:
- Navidrome + alle Subsonic/OpenSubsonic-Server (Gonic, Airsonic, Ampache, Nextcloud Music …)
- Jellyfin
- Emby
- Plex

Funktionen werden je nach Server-Fähigkeit ein-/ausgeblendet.

## 3. Oberfläche

- Aufbau wie Spotify, eigener Name/Logo/Akzentfarbe.
- Player-, Album- und Playlist-Seiten färben sich nach dem Cover.
- Themes: Dunkel, Echtes Schwarz (AMOLED), Hell, System. Bei „System“ einstellbar, ob der Dunkel-Fall „Dunkel“ oder „Echtes Schwarz“ nutzt.
- Untere Tabs: **Home, Suche, Bibliothek, Downloads, Statistik**.
- Sprachen: Deutsch + Englisch (alle Texte ausgelagert).

## 4. Warteschlange (Spotify-Modell)

- Zwei Bereiche: **„Als Nächstes in der Warteschlange“** (eigene Liste) und **„Als Nächstes von: <Album/Playlist>“** (Kontext).
- Einzige Aktion: **„Zur Warteschlange“** → ans Ende der eigenen Warteschlange, also vor dem Rest des Kontexts. Kein „Als Nächstes spielen“.
- Beim Start eines neuen Albums/einer Playlist **bleibt die eigene Warteschlange erhalten**.
- Titel in der Warteschlange sind per Drag & Drop **frei verschiebbar**.
- Shuffle/Wiederholen wie Spotify (Shuffle betrifft den Kontext, nicht die eigene Warteschlange).

## 5. Wisch-Gesten (konfigurierbar)

- Pro Richtung **kurzer und langer Swipe** → bis zu 4 Aktionen.
- Orte: Titel-Zeilen in Listen, Album-/Playlist-Zeilen, Warteschlange, Mini-Player & Cover.
- Aktionen u. a.: zur Warteschlange, Favorit, herunterladen, zur Playlist hinzufügen, entfernen (Warteschlange), Titel wechseln / Player öffnen (Mini-Player).

## 6. Player

- Lückenlose Wiedergabe (immer).
- Synchronisierte Songtexte (wenn Server sie liefert).
- Crossfade (einstellbar, bei Titeln desselben Albums automatisch aus).
- Sleep-Timer (Minuten oder Ende des Titels, sanftes Ausblenden).
- **Kein** Tempo-Regler, daher keine Tonhöhenerhaltung.

## 7. Klang

- Parametrischer EQ (Frequenz, Gain, Q, Filtertyp).
- Einfacher 10-Band-Modus zusätzlich.
- AutoEQ-Kopfhörerprofile importierbar.
- EQ-Profil je Ausgabegerät (automatischer Wechsel bei Bluetooth/Kabel/Lautsprecher/Auto).
- Kompressor.
- ReplayGain (Track/Album) mit Preamp und Clipping-Schutz.
- Keine fertigen Presets.

## 8. Qualität & Formate

- Getrennte Einstellungen: **Streaming WLAN / Streaming mobil / Download**.
- Formate: Opus, AAC, MP3, Original/verlustfrei (Umwandlung durch den Server).

## 9. Offline & Downloads

- Downloads im Hintergrund (WorkManager / Foreground-Service), mit Fortsetzen nach Abbruch, Download-Warteschlange.
- Automatischer Offline-Modus (kein Netz / bestimmtes WLAN verlassen).
- Einstellbar: in mobilen Daten nur Offline-Inhalte anzeigen – oder abschaltbar, sodass bei jeder Verbindung normal online gearbeitet wird.
- Speicherlimit + Speicherübersicht.
- Downloads nur im WLAN (einstellbar).
- Speicherort wählbar (intern / SD-Karte).
- Auto-Sync, jede Option einzeln einstellbar: Favoriten automatisch, heruntergeladene Playlists synchron halten, Smart-Playlists automatisch, Streaming-Cache mit Größenlimit.

## 10. Smart-Playlists (in der App)

- Editor mit verschachtelten **UND/ODER-Gruppen**, Sortierung, Limit (Anzahl/Dauer), plus Vorlagen (z. B. „Vergessene Favoriten“, „Neu hinzugefügt“).
- Berechnung lokal aus Bibliothek + eigener Hörhistorie, funktioniert offline.
- **Pro Playlist wählbar**, ob das Ergebnis zusätzlich als normale Playlist auf den Server gespiegelt wird.

## 11. Android-Integration

Immer: MediaSession/`MediaLibraryService`, Medien-Benachrichtigung, Sperrbildschirm, Bluetooth-/Kopfhörer-Tasten, Ausgabe-Umschalter, Weiterspielen nach Neustart (Media Resumption) – damit Android 17 die App als Musik-App erkennt.

Zusätzlich:
- Android Auto (Durchsuchen + Sprachsuche)
- Homescreen-Widgets wie bei Spotify:
  - Klein: Cover + Play/Pause
  - Mittel: Cover, Titel/Künstler, Zurück/Play/Weiter, Favorit
  - Groß: zusätzlich Fortschritt, Shuffle/Wiederholen, Hintergrund in Cover-Farbe
  - Schnellzugriff: zuletzt gehörte / angeheftete Playlists, Alben, Smart-Playlists

## 12. Sonstiges

- Scrobbling nur an den eigenen Server, offline gesammelt und nachgereicht. Eigene lokale Hörhistorie für Statistik und Smart-Playlists.
- Statistik-Tab: Verlauf, Top-Künstler/-Alben/-Titel, Heatmap.
- Verteilung: erstmal nur eigene APK.
- Lizenz: GPL-3.0.
