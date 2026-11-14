# Stand der Umsetzung

Die vollständige Zielbeschreibung steht in [SPEC.md](SPEC.md). Diese Datei hält fest, was davon gebaut ist, was wirklich getestet wurde und was offen bleibt.

Die App heißt **Nocturne** (App-ID `io.github.lama072004.nocturne`). Das interne Dart-Paket heißt weiterhin `musicplayer`; das sieht kein Nutzer.

Stand: 14.11.2026

## Projektaufbau

| Ordner | Inhalt |
|---|---|
| `lib/` | Flutter-Oberfläche (Dart) |
| `lib/src/rust/` | Automatisch erzeugte Dart-Bindungen. **Nicht von Hand ändern.** |
| `rust/src/api/` | Schnittstelle, die Dart sieht |
| `rust/src/core/` | Warteschlange, Datenbank, Smart-Playlist-Regeln, Speicher, Inhaltsbaum für Auto/Widgets |
| `rust/src/server/` | Server-Adapter: Subsonic/Navidrome, Jellyfin, Emby, Plex |
| `rust/src/dsp/` | Klangbearbeitung (EQ, Kompressor, ReplayGain, Effektkette) |
| `rust/src/jni_bridge.rs` | Direkte Verbindung Kotlin → Rust (Audio-Pfad, Dienst ohne Oberfläche) |
| `rust_builder/` | Baut die Rust-Bibliothek beim Flutter-Build mit (Cargokit) |
| `android/app/src/main/kotlin/` | Media3-Wiedergabe, Downloads, Effekte, Crossfade, Widgets, Brücken zu Flutter |
| `assets/logo.svg` | Logo als Vektor |
| `docs/` | Ziel (SPEC), Stand (diese Datei), Modell-Aufteilung (HARNESS), UI-Recherche und -Plan |

## Nach Änderungen an `rust/src/api/`

```bash
flutter_rust_bridge_codegen generate
```

## Tests

```bash
cd rust && cargo test
```

```bash
flutter test
```

```bash
cd android && ./gradlew :app:testDebugUnitTest
```

Stand der automatischen Tests: Rust 70 bestanden (2 Live-Tests gegen Demo-Server zusätzlich, von Hand zu starten), Dart 5, Kotlin 7 (Rechenkern der Überblendung). `flutter analyze` meldet nichts.

## Bauen

```bash
flutter build apk --debug --target-platform android-arm64,android-x64
```

Bricht Gradle unter Windows mit „Unable to establish loopback connection“ ab, vorher diese Variable setzen:

```bash
export JAVA_TOOL_OPTIONS="-Djdk.net.unixdomain.tmpdir=D:/tmp_gradle"
```

Ein vollständiger Bau dauert etwa 15 Minuten, wenn sich Rust geändert hat (drei Prozessorarten), sonst etwa 4 Minuten.

## Fertig

**Grundlage**
- Rust-Kern mit Brücke zu Flutter; die Rust-Bibliothek ist auf 16-KB-Speicherseiten ausgerichtet (nötig für neue Geräte)
- Anmeldung an Navidrome/Subsonic, Jellyfin, Emby und Plex (HTTPS zuerst, HTTP im Heimnetz möglich)
- Anmeldedaten liegen verschlüsselt auf dem Gerät (Schlüssel im Android-Keystore)
- Deutsch und Englisch
- Themes: Dunkel, Echtes Schwarz, Hell, System (mit wählbarer Dunkel-Variante), frei wählbare Akzentfarbe
- Name, adaptives App-Icon (auch einfarbig für „Themed Icons“) und Startbild
- Startet die App nicht, erscheint eine Fehlerseite statt eines ewigen Startbilds

**Oberfläche (nach dem Vorbild von Spotify, siehe UI_PLAN.md)**
- Drei Reiter: Start, Suche, Bibliothek; Profil mit Statistik oben links
- Start: Begrüßung, Schnellzugriffe, zuletzt gehört, neu hinzugefügt, Top-Titel, Smart-Playlists
- Suche: Genre-Kacheln, zuletzt gesucht, Ergebnisse mit Filter (Titel, Alben, Künstler)
- Bibliothek: Filter-Chips (Playlists, Alben, Künstler, Downloads, Favoriten, Smart), Sortierung, Liste oder Raster; die Auswahl bleibt gemerkt
- Album-, Playlist- und Künstler-Seiten. Künstler: „Beliebt“, „Alben“, „Enthalten auf“ (Sampler)
- Jede Titel-Zeile: Tippen spielt, Drei Punkte oder langes Drücken öffnet das Menü, Wischen löst einstellbare Abkürzungen aus (mit „Rückgängig“)
- Mini-Player und Vollbild-Player mit Farbe aus dem Cover
- Leere Zustände führen immer zu einem nächsten Schritt

**Wiedergabe**
- Media3 mit MediaSession (Benachrichtigung, Sperrbildschirm, Bluetooth-Tasten, Anmeldung als Musik-App)
- Warteschlange nach dem Spotify-Modell: nur „Zur Warteschlange“; eigene Einträge bleiben erhalten, wenn ein neues Album startet; frei verschiebbar
- Shuffle und Wiederholen (aus / alle / Titel), lückenlose Wiedergabe
- Überblenden (Crossfade) 1–12 Sekunden, Standard aus
- Sleep-Timer (Minuten oder Titelende, mit Ausblenden)
- Songtexte, mitlaufend, wenn der Server Zeitstempel liefert
- Weiterspielen nach einem Neustart: Warteschlange und Position werden gemerkt; „Play“ im Mini-Player, im Widget oder am Kopfhörer setzt fort
- Läuft Musik ohne geöffnete App weiter, führt der Dienst die Warteschlange selbst nach

**Android-Integration**
- Vier Homescreen-Widgets: Player klein (2×1), Player (4×1), Player groß (4×2), Schnellzugriff (4×2, zuletzt gehörte Alben). Sie zeichnen nur bei echten Änderungen neu (Akku)
- Inhaltsbaum für Android Auto und Sprachassistenten (Zuletzt gehört, Playlists, Alben, Heruntergeladen; Suche; Abspielen per ID)

**Klang**
- Effektkette in Rust direkt im Audio-Pfad von ExoPlayer
- Parametrischer Equalizer (bis 16 Bänder) mit Kurven-Anzeige, einfacher 10-Band-Modus
- AutoEQ: Kopfhörer in der öffentlichen Datenbank suchen oder Profil-Text einfügen
- Eigenes EQ-Profil je Ausgabegerät, automatischer Wechsel
- Kompressor, ReplayGain (Titel/Album) mit Vorverstärkung und Schutz vor Übersteuerung
- Sind alle Effekte und Crossfade aus, nutzt Android stromsparendes Hardware-Offload

**Offline**
- Downloads im Hintergrund, Offline-Wiedergabe, Streaming-Cache mit einstellbarer Größe
- Offline-Modus: automatisch ohne Netz, wahlweise auch in mobilen Daten
- Downloads nur im WLAN, Speicherlimit, SD-Karte (Wechsel mit Rückfrage; vorhandene Downloads werden dabei entfernt)
- Qualität getrennt für WLAN, mobile Daten und Downloads (Opus, AAC, MP3, Original)
- Auto-Sync, einzeln schaltbar: Favoriten, vorgemerkte Playlists, Smart-Playlists
- Lokale Kopie der Bibliothek: Alben, Künstler, Genres und Suche gehen sofort und offline

**Smart-Playlists**
- Regel-Editor mit UND/ODER und Untergruppen, Live-Trefferzahl, Vorlagen
- Sortierung, Limit nach Anzahl und Minuten
- Pro Playlist wählbar: auf den Server spiegeln, automatisch herunterladen

**Statistik**
- Hörverlauf lokal in SQLite; gezählt wird im Wiedergabe-Dienst (auch ohne geöffnete App)
- Wiedergaben, Hörzeit, Top-Titel/-Künstler/-Alben, Tageszeiten, Verlauf; Zeitraum wählbar
- Scrobbling an den eigenen Server, offline Gehörtes wird nachgereicht

## Was getestet ist

Im Android-Emulator `nocturne_test` (x86_64, 4-KB-Seiten) gegen `demo.navidrome.org`:

- Anmeldung; alle Seiten der Oberfläche in hellem und dunklem Design
- Wiedergabe, Benachrichtigung und System-Medienkarte mit Cover, Medientasten
- Warteschlange: Hinzufügen per Wischen (kurze Bestätigung, höchstens 1 s, ohne „Rückgängig“), Entfernen mit Rückgängig, Verschieben, Leeren
- Vollbild-Player: Steuerung direkt unter dem Cover (Einhand-Bedienung), auch auf kleinem Bildschirm (720×1280) ohne Überlauf
- App hart beendet → neu geöffnet → Mini-Player zeigt den letzten Titel, „Play“ setzt an der gemerkten Stelle fort
- Oberfläche aus den letzten Apps gewischt, Musik läuft weiter, Titelwechsel per Medientaste → Warteschlange stimmt nach dem Wiederöffnen
- Widgets „Player groß“ und „Schnellzugriff“ auf dem Startbildschirm: Anzeige, Knöpfe, Start ohne laufende App, Album aus dem Schnellzugriff abspielen
- Offline: Titel heruntergeladen, Flugmodus an, nie zuvor gestreamter Titel spielt
- Crossfade 5 s: Wechsel 5 s vor Titelende, Position läuft durch, keine Aussetzer im Log
- Sleep-Timer 5 Minuten
- Verlauf: jeder gespielte Titel steht einmal pro Wiedergabe darin
- Equalizer eingeschaltet bei laufender Wiedergabe (stabil)

Statisch geprüft: 16-KB-Ausrichtung der Rust-Bibliothek in der APK (`zipalign -c -P 16`).

## Was NICHT getestet ist

- **Kein echtes Gerät.** Insbesondere nicht: ein Gerät mit 16-KB-Speicherseiten (das 16-KB-Emulator-Abbild startet auf diesem PC nicht), Akkuverbrauch, Bluetooth, Hardware-Offload.
- **Klang nach Gehör:** Equalizer, Kompressor, ReplayGain und Crossfade sind gerechnet und per Test bzw. Log geprüft, aber nie angehört worden.
- **Android Auto** in einem Auto oder im Auto-Simulator, **Sprachsuche**. Der Inhaltsbaum selbst läuft (das Schnellzugriff-Widget nutzt denselben Weg).
- **Jellyfin:** nur der Rust-Adapter gegen `demo.jellyfin.org`, nicht die App. **Emby, Plex:** nur Unit-Tests mit nachgebauten Antworten.
- **SD-Karte** (der Emulator hat keine), Widgets „Player klein“ und „Player“ auf dem Startbildschirm (nur in der Vorschau gesehen).

## Bekannte Einschränkungen

- Crossfade: Kurz nach Start, Spulen und Titelwechsel ist die volle Dauer noch nicht aufgebaut; wechselt der Titel in dieser Zeit, fällt die Überblendung kürzer aus. Der letzte Titel der Warteschlange und „Sleep-Timer bis Titelende“ klingen ohne Überblendung aus. Zwischen Titeln mit verschiedener Abtastrate wird nicht überblendet.
- ReplayGain wirkt bei lückenlosen Übergängen einige Millisekunden zu spät (Tiefe des Tonpuffers).
- Der Wechsel des Download-Speicherorts wirkt erst nach einem App-Neustart.
- Auto-Sync lädt Fehlendes nach, löscht aber nichts, was aus Favoriten oder Playlists entfernt wurde.
- Smart-Playlist-Gruppen gehen eine Ebene tief (Regeln plus Untergruppen).
- Die Effektkette verarbeitet Stereo und Mono; Mehrkanal-Ton läuft unverändert durch.
- Nach dem Ende der Warteschlange beginnt „Play“ den letzten Titel von vorn (nicht das Album).

## Bewusst nicht geplant

- **Chromecast** und eine **eigene Wear-OS-App**: am 14.11.2026 vom Nutzer gestrichen. Eine Uhr kann die Wiedergabe trotzdem über die Mediensteuerung des Systems bedienen; das kommt ohne eigenen Code von der MediaSession.

## Noch offen

- Server-Art bei der Anmeldung automatisch erkennen
- Statistik teilen (Bild)
