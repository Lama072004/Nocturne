# Nocturne

Deutsch | [English](#english)

Ein Musik-Client für den eigenen Server, der aussieht und sich bedient wie Spotify. Die Musik bleibt auf deinem Server, die App ist nur die Oberfläche dafür. Unterstützt werden **Navidrome/Subsonic**, **Jellyfin**, **Emby** und **Plex**. Die App läuft auf **Android**.

> **Stand:** Die App ist funktionsfähig und im Emulator getestet, aber noch nicht auf einem breiten Gerätepark. Was fertig ist, was getestet wurde und was fehlt, steht ehrlich in [docs/STATUS.md](docs/STATUS.md). Das Ziel steht in [docs/SPEC.md](docs/SPEC.md).

## Funktionen

**Bedienung**
- Drei Reiter: Start, Suche, Bibliothek; Profil mit Statistik
- Warteschlange wie bei Spotify: „Zur Warteschlange“ bleibt erhalten, auch wenn ein neues Album startet; frei per Drag & Drop verschiebbar
- Einstellbare Wischgesten (kurz und lang, je Richtung) auf Titeln, Alben, Warteschlange, Mini-Player und Cover
- Themes: Dunkel, Echtes Schwarz, Hell, System; Farben aus dem Cover; frei wählbare Akzentfarbe
- Deutsch und Englisch

**Wiedergabe**
- Lückenlos, Überblenden (Crossfade), Shuffle, Wiederholen, Sleep-Timer, mitlaufende Songtexte
- Weiterspielen nach dem Neustart
- Kein Tempo- oder Tonhöhen-Regler (bewusst)

**Klang**
- Parametrischer Equalizer, einfacher 10-Band-Modus, AutoEQ-Import
- Eigenes Profil je Ausgabegerät, Kompressor, ReplayGain
- Ohne aktive Effekte nutzt Android stromsparendes Hardware-Offload

**Offline**
- Downloads im Hintergrund, WLAN-only, Speicherlimit, SD-Karte
- Automatischer Offline-Modus, Qualität getrennt für WLAN, mobile Daten und Downloads (Opus, AAC, MP3, Original)
- Auto-Sync für Favoriten, Playlists und Smart-Playlists

**Smart-Playlists**
- Regeln mit UND/ODER, Vorlagen, wahlweise auf den Server gespiegelt

**Android**
- Wird als Musik-App erkannt (MediaSession, Media Resumption)
- Homescreen-Widgets: Player klein, Player, Player groß, Schnellzugriff
- Inhaltsbaum für Android Auto und Sprachsuche
- Scrobbling nur an den eigenen Server

**Bewusst nicht dabei:** Chromecast und eine eigene Wear-OS-App.

## Technik

| Schicht | Aufgabe |
|---|---|
| Flutter (`lib/`) | Oberfläche |
| Rust (`rust/`) | Server-Anbindung, Warteschlange, Datenbank, Smart-Playlists, Klangeffekte |
| Kotlin + Media3 (`android/`) | Wiedergabe, MediaSession, Downloads, Widgets, Android Auto |

Flutter und Rust sprechen über `flutter_rust_bridge`, Kotlin und Rust über JNI. Die Rust-Bibliothek ist auf 16-KB-Speicherseiten ausgerichtet (nötig für neue Geräte).

## Entwickeln

Voraussetzungen: Flutter 3.35, Rust mit `cargo-ndk`, Android SDK und NDK 27, JDK 21.

```bash
flutter pub get
flutter run
```

Tests:

```bash
cd rust && cargo test
```

```bash
flutter test
```

```bash
cd android && ./gradlew :app:testDebugUnitTest
```

APK bauen (Handy und Emulator):

```bash
flutter build apk --debug --target-platform android-arm64,android-x64
```

Nach Änderungen an `rust/src/api/` die Bindungen neu erzeugen:

```bash
flutter_rust_bridge_codegen generate
```

Weitere Hinweise (Windows, Emulator, Fallstricke) stehen in [CLAUDE.md](CLAUDE.md) und [docs/STATUS.md](docs/STATUS.md).

## Lizenz

GPL-3.0.

---

## English

# Nocturne

A music client for your own server that looks and feels like Spotify. Your music stays on your server; the app is just the interface. It supports **Navidrome/Subsonic**, **Jellyfin**, **Emby** and **Plex**, and runs on **Android**.

> **Status:** The app works and has been tested in an emulator, but not yet on a wide range of devices. What is done, what has been tested and what is missing is listed honestly in [docs/STATUS.md](docs/STATUS.md) (German). The goal is in [docs/SPEC.md](docs/SPEC.md) (German).

### Features

**Interface**
- Three tabs: Home, Search, Library; profile with statistics
- Spotify-style queue: "Add to queue" is kept when a new album starts; freely reorderable by drag and drop
- Configurable swipe gestures (short and long, per direction) on tracks, albums, queue, mini player and cover
- Themes: Dark, True Black, Light, System; colors from the cover; user-selectable accent color
- German and English

**Playback**
- Gapless, crossfade, shuffle, repeat, sleep timer, synced lyrics
- Resume after restart
- No tempo or pitch control (on purpose)

**Sound**
- Parametric equalizer, simple 10-band mode, AutoEQ import
- Per-output-device profile, compressor, ReplayGain
- With no active effects, Android uses power-saving hardware offload

**Offline**
- Background downloads, Wi-Fi only, storage limit, SD card
- Automatic offline mode; quality set separately for Wi-Fi, mobile data and downloads (Opus, AAC, MP3, original)
- Auto-sync for favorites, playlists and smart playlists

**Smart playlists**
- AND/OR rules, templates, optionally mirrored to the server

**Android**
- Recognized as a music app (MediaSession, media resumption)
- Home screen widgets: small player, player, large player, quick access
- Content tree for Android Auto and voice search
- Scrobbling to your own server only

**Not included on purpose:** Chromecast and a dedicated Wear OS app.

### Tech

| Layer | Job |
|---|---|
| Flutter (`lib/`) | UI |
| Rust (`rust/`) | Server access, queue, database, smart playlists, sound effects |
| Kotlin + Media3 (`android/`) | Playback, MediaSession, downloads, widgets, Android Auto |

Flutter and Rust talk through `flutter_rust_bridge`, Kotlin and Rust through JNI. The Rust library is aligned to 16 KB memory pages (required on newer devices).

### Development

Requirements: Flutter 3.35, Rust with `cargo-ndk`, Android SDK and NDK 27, JDK 21.

```bash
flutter pub get
flutter run
```

Tests:

```bash
cd rust && cargo test
```

```bash
flutter test
```

```bash
cd android && ./gradlew :app:testDebugUnitTest
```

Build an APK (phone and emulator):

```bash
flutter build apk --debug --target-platform android-arm64,android-x64
```

After changing `rust/src/api/`, regenerate the bindings:

```bash
flutter_rust_bridge_codegen generate
```

### License

GPL-3.0.
