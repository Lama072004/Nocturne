# Modell-Harness

Dieses Projekt verteilt Arbeit auf verschiedene Claude-Modelle, statt alles mit dem teuersten zu machen. Jede Art von Aufgabe geht an das Modell, das sie zuverlässig **und** am günstigsten löst.

## Woher die Regeln stammen

Grundlage ist die Modellwahl-Vorlage `automodel` (Stand 2026-11-08) mit diesen Messwerten:

| Modell | Preis $/1M (ein/aus) | Messwerte | Folgerung |
|---|---|---|---|
| Haiku 5.5 | 0,10 / 0,50 | Terminal-Bench 4.0: 39,2 %, OSWorld 2.1: 72,4 % | Suchen, Lesen, Zusammenfassen. Nicht für schwieriges Coding. |
| Sonnet 5.5 | 2 / 10 | Terminal-Bench 4.0: 70,6 % (vor Opus mit 66,4 %) | Standard für klar umrissene Coding-Aufgaben. |
| Opus 5.5 | 4 / 20 | beste Zuverlässigkeit in langen Agenten-Läufen | Architektur, viele Dateien, riskante Stellen, Review. |
| Fable 5.2 | – | nicht in der Vorlage gemessen | Nur als Leitung: zerlegt, entscheidet, prüft Ergebnisse. |

Zwei Messergebnisse bestimmen die Effort-Stufen:

- **Sonnet nie auf Max.** Dort braucht es rund 193k Ausgabe-Tokens pro Aufgabe gegen rund 119k bei Opus und wird damit teurer als Opus (ca. 7,60 $ gegen 5,98 $ pro Aufgabe).
- **Sonnet auf Low/Medium ist der günstigste gute Punkt.** Braucht eine Aufgabe mehr als High, geht sie an Opus.

> Hinweis: Das Morpheus-Video selbst ließ sich beim Anlegen nicht abrufen. Weichen seine Ergebnisse von der Tabelle oben ab, genügt es, die Tabelle und die `model:`-Zeilen in `.claude/agents/` anzupassen.

## Rollen

Die Rollen liegen als Subagenten in `.claude/agents/`. Das Modell steht jeweils in der Kopfzeile der Datei.

| Rolle | Modell | Effort | Wofür |
|---|---|---|---|
| `scout` | Haiku 5.5 | – | Dateien finden, Code lesen, Logs durchsuchen, zusammenfassen. Ändert nichts. |
| `builder` | Sonnet 5.5 | Medium | Klar beschriebene Umsetzung: ein Bildschirm, ein Bugfix, eine Funktion, Tests. |
| `hard-builder` | Sonnet 5.5 | High | Schwierig, aber gut spezifiziert (z. B. ein Server-Adapter nach API-Doku). |
| `architect` | Opus 5.5 | High | Viele Dateien zugleich, Nebenläufigkeit, Audio-Pfad, JNI, Architektur. |
| `reviewer` | Opus 5.5 | High | Prüft riskanten Code: `unsafe`, Anmeldedaten, Audio-Thread, Migrationen. Ändert nichts. |
| `ui-tester` | Sonnet 5.5 | Medium | Bedient die App im Emulator (adb), macht Bildschirmfotos, meldet Fehler. |
| Leitung | Fable 5.2 | – | Hauptsitzung: plant, verteilt, prüft jedes Ergebnis selbst nach. |

## Regeln für die Leitung

1. **Billigstes passendes Modell zuerst.** Suchen und Lesen gehen an `scout`, nicht an ein großes Modell.
2. **Eskalieren nur bei Nachweis**, nicht auf Verdacht:
   - Tests oder Build schlagen nach 2 Versuchen weiter fehl,
   - das Modell wiederholt dieselbe falsche Annahme,
   - die Aufgabe betrifft mehr als etwa 5 Dateien zugleich.

   Dann **Modell wechseln** (Sonnet → Opus bei gleichem Effort), nicht erst den Effort hochdrehen.
3. **Ein Schreiber pro Dateibereich.** Zwei Agenten bearbeiten nie dieselben Dateien. Die Bereiche:
   - `rust/src/server/` + Login in `rust/src/api/library.rs`
   - `rust/src/dsp/` + `rust/src/jni_bridge.rs`
   - `android/`
   - `lib/` (Flutter)
4. **Ein Emulator, ein Build.** Nur ein Agent zur selben Zeit baut die APK oder bedient den Emulator. Rust-Tests (`cargo test`) dürfen parallel laufen.
5. **Nichts gilt als fertig, bevor die Leitung es geprüft hat**: Diff lesen, Tests laufen lassen, im Emulator ansehen.
6. **Riskanter Code bekommt ein Review von `reviewer`**, bevor er als fertig gilt.

## Auftrags-Vorlage

Jeder Auftrag an einen Agenten folgt diesem Muster. Klare Aufträge sparen Rückfragen und damit Tokens.

```
Ziel: ...
Kontext/Dateien: (Pfade)
Grenzen: (welche Dateien NICHT angefasst werden dürfen)
Fertig wenn: (prüfbares Kriterium)
Verifikation: (Befehl, den der Agent selbst ausführt)
Antwort: (Format und Länge, z. B. "geänderte Dateien + offene Punkte, max. 15 Zeilen")
```

## Befehle zur Verifikation

```bash
cd rust && cargo test
```

```bash
flutter analyze
```

```bash
flutter test
```
