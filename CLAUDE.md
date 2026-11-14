# Nocturne

Musik-Client für Navidrome und andere Server. Flutter-UI (`lib/`), Rust-Kern (`rust/`), Android-Teil mit Media3 (`android/`).

- Ziel: `docs/SPEC.md`
- Stand und offene Punkte: `docs/STATUS.md`
- **Arbeitsteilung auf Modelle: `docs/HARNESS.md`** (vor jeder größeren Aufgabe lesen und anwenden)

## Regeln

- Kommentare auf Deutsch und reichlich: Jede Datei erklärt oben ihren Zweck, wichtige Stellen das WARUM.
- Akku schonen: keine Hintergrund-Timer ohne Not; Hardware-Offload bleibt an, solange keine Klangeffekte aktiv sind.
- `lib/src/rust/` ist erzeugter Code. Nach Änderungen an `rust/src/api/` neu erzeugen: `flutter_rust_bridge_codegen generate`.
- Getestet wird im Android-Emulator `nocturne_test`, gegen den öffentlichen Demo-Server `demo.navidrome.org` (Benutzer und Passwort `demo`). Das Abbild `nocturne_16k` (16-KB-Speicherseiten) startet auf diesem PC nicht; die 16-KB-Ausrichtung wird deshalb an der APK geprüft (`zipalign -c -P 16 -v 4`).
- Klang lässt sich im Emulator nicht anhören. Was am Ton etwas ändert (Effekte, Crossfade), braucht einen Unit-Test für die Rechnung und eine Prüfung im Log (`adb logcat`, keine „underrun“-Meldungen).
- Die Rust-Bibliothek muss auf 16 KB ausgerichtet bleiben (`rust/build.rs`), sonst startet die App auf neuen Geräten nicht.

## Prüfen

```bash
cd rust && cargo test
```

```bash
flutter analyze
```

```bash
cd android && ./gradlew :app:testDebugUnitTest
```

```bash
flutter build apk --debug --target-platform android-arm64,android-x64
```

Bricht Gradle unter Windows mit "Unable to establish loopback connection" ab, vorher setzen:

```bash
export JAVA_TOOL_OPTIONS="-Djdk.net.unixdomain.tmpdir=D:/tmp_gradle"
```
