# Umbauplan Oberfläche

Grundlage: [UI_RESEARCH.md](UI_RESEARCH.md) (Vergleich mit Spotify und anderen Clients, Nutzerkritik) und die Entscheidungen aus [SPEC.md](SPEC.md). Wo beides sich widerspricht, gilt SPEC.md, weil es die ausdrücklichen Wünsche des Nutzers enthält.

Abweichung von der Recherche: Es gibt **kein „Als Nächstes spielen“**. Der Nutzer hat ausdrücklich nur „Zur Warteschlange“ gewählt.

## 1. Navigation

- **Drei Tabs:** Home · Suche · Bibliothek. Beschriftung immer sichtbar.
- Erneutes Tippen auf den aktiven Tab springt zu dessen Startseite.
- **Profil-Knopf oben links** auf allen drei Tabs (Kreis mit Anfangsbuchstabe des Benutzers). Er öffnet die Profilseite.
- Statistik, Downloads und Einstellungen sind keine Tabs mehr. Sie liegen im Profil.
- Über der Tab-Leiste: Offline-Hinweis (nur wenn offline) und der Mini-Player.

## 2. Profil (`ProfileScreen`)

Reihenfolge von oben nach unten:

1. Kopf: Benutzername, Server-Adresse, Online/Offline.
2. **Deine Statistik** (Kurzfassung direkt sichtbar: Hörzeit und Wiedergaben der letzten 4 Wochen; Tippen öffnet die ganze Statistik).
3. Verlauf
4. Downloads verwalten (Fortschritt, Speicher, Fehler)
5. Klang (Equalizer, Kompressor, ReplayGain)
6. Einstellungen
7. Abmelden

Statistik-Seite: Standard-Zeitraum **4 Wochen**. Reihenfolge: Top-Künstler, Top-Titel, Top-Alben, dann Hörzeit, Tageszeiten, Aktivität.

## 3. Home

Feste Reihen, kein wechselnder Feed:

1. Begrüßung nach Tageszeit + Profil-Knopf.
2. **Schnellzugriff-Raster (2 Spalten, bis 8 Kacheln):** zuletzt gehörte Alben. Kachel = kleines Cover + Name.
3. „Zuletzt gehört“ (waagrechte Reihe)
4. „Neu hinzugefügt“ (waagrechte Reihe)
5. „Deine Top-Titel“ (aus der eigenen Statistik, nur wenn Daten vorhanden)
6. „Smart-Playlists“ (nur wenn vorhanden)

Jede Reihe lädt für sich. Schlägt eine fehl, zeigt nur diese Reihe den Fehler mit „Erneut versuchen“.

## 4. Suche

- Ohne Eingabe: „Zuletzt gesucht“ (Tippen füllt das Suchfeld, spielt nichts ab; × entfernt den Eintrag).
- Mit Eingabe: Chips Alle · Titel · Alben · Künstler. Ergebnisse gruppiert. Verzögerung 250 ms.
- Keine Treffer: „Keine Treffer für »xyz«“.

## 5. Bibliothek

- Filter-Chips: Playlists · Alben · Künstler · Heruntergeladen · Favoriten · Smart. Ist ein Chip aktiv, steht davor ein × zum Zurücksetzen.
- Darunter eine Zeile: links Sortierung (Name / Zuletzt hinzugefügt / Künstler / Jahr, je nach Chip), rechts Umschalter Raster/Liste. **Beides wird pro Chip gespeichert.**
- Ganz oben fest: „Lieblingssongs“ (Favoriten-Titel).
- Alben und Künstler kommen aus der **lokalen Titel-Kopie** (sofort da, offline nutzbar). Ist sie leer, wird sie beim ersten Öffnen automatisch geladen, mit Fortschrittsanzeige.
- Lupe in der Kopfzeile filtert die sichtbare Liste lokal.
- Alle Listen mit fester Zeilenhöhe (flüssiges Scrollen bei großen Sammlungen).

## 6. Zeilen und Kontextmenü

- Jede Titel-, Album- und Playlist-Zeile hat rechts einen **Drei-Punkte-Knopf**. Langes Drücken öffnet dasselbe Menü.
- Das Menü ist ein Blatt von unten (`context_menu.dart`), überall mit gleicher Reihenfolge: Zur Warteschlange · Favorit · Zur Playlist · Herunterladen bzw. Download entfernen · Zum Album · Zum Künstler · (in der Warteschlange: Entfernen).
- Wischgesten bleiben als Abkürzung. Standard: kurz rechts = Zur Warteschlange, kurz links = Favorit, lang links = Herunterladen.
- Jede Aktion bestätigt sich mit einem kurzen Hinweis und einem leichten Vibrieren.
- Zustand an der Zeile sichtbar: Herz (Favorit), Haken (heruntergeladen), Fortschrittskreis (lädt).
- Offline: nicht heruntergeladene Titel halb durchsichtig; Tippen zeigt „Nicht offline verfügbar“.

## 7. Mini-Player

- Tippen oder nach oben wischen öffnet den Player.
- Waagrecht wischen wechselt den Titel, aber nur mit deutlicher Bewegung und nicht vom Bildschirmrand aus (dort gehört die Geste dem System-Zurück).
- Play/Pause rechts, mindestens 48 × 48. Dünne Fortschrittslinie unten.
- Hintergrund leicht in der Cover-Farbe.

## 8. Player (Vollbild)

Alles Wichtige in der unteren Hälfte, mit einer Hand erreichbar:

1. Kopf: Pfeil nach unten · „Wiedergabe von: <Quelle>“ · Drei-Punkte-Menü (Sleep-Timer, Klang, Zum Album, Zum Künstler, Zur Playlist).
2. Cover (waagrecht wischen = Titel wechseln).
3. Titel + Künstler, rechts das Herz.
4. Fortschritt mit Zeiten.
5. Shuffle · Zurück · Play (64) · Weiter · Wiederholen.
6. Unten: links Ausgabegerät, rechts Songtext · Warteschlange.

Nach unten wischen schließt den Player. Hintergrund als Verlauf aus der Cover-Farbe, abgedunkelt, damit Text lesbar bleibt.

## 9. Warteschlange

- Abschnitte: Läuft gerade · Als Nächstes in der Warteschlange · Als Nächstes von: <Quelle>.
- „Leeren“ ist immer sichtbar, wenn die eigene Warteschlange nicht leer ist.
- Ziehgriff rechts zum Umsortieren, auch bei Shuffle.
- Nach links wischen entfernt, mit „Rückgängig“.
- Shuffle und Wiederholen zusätzlich oben in der Kopfzeile.
- Leer: „Nichts in der Warteschlange“ mit Knopf „Zur Bibliothek“.

## 10. Leere Zustände und Fehler

- Jeder leere Zustand sagt, was man als Nächstes tun kann, und bietet den passenden Knopf an.
- Fehler nennen, was passiert ist und was zu tun ist. Keine rohen Meldungen mit Internet-Adressen (dort stecken Anmeldedaten drin).
- Anmeldung: Adresse ohne „http://“ erlaubt; Servertyp wird automatisch erkannt, wenn möglich.

## 11. Barrierefreiheit und Bedienung

- Alle Knöpfe mindestens 48 × 48 und mit Beschriftung für Vorlese-Funktionen.
- Text auf Cover-Farben immer auf abgedunkeltem Grund.
- Jede Geste ist auch über ein Menü erreichbar.
