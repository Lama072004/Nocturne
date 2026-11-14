# UI-Recherche für die Neugestaltung von Nocturne

Stand: November 2026. Grundlage: Websuche in Fachartikeln, offiziellen Design-Richtlinien, Spotify-Community, GitHub-Diskussionen, Lemmy/Foren und App-Store-Seiten. Reddit selbst ließ sich nicht direkt abrufen; Reddit-Meinungen sind nur über Spiegel/Zitate in anderen Quellen erfasst.

**Kennzeichnung:** **[Beleg]** = in der verlinkten Quelle gefunden (sinngemäß wiedergegeben, keine wörtlichen Zitate außer in Anführungszeichen). **[Einschätzung]** = eigene Bewertung/Ableitung, nicht belegt.

---

## Kurzfazit

1. Spotify kommt seit Jahren mit **3 Haupt-Tabs** aus (Home, Suche, Bibliothek); ein 4. Tab ("Create") wurde so stark kritisiert, dass er abschaltbar wurde. Statistik liegt seit Nov. 2025 **im Profil-Menü**, nicht als Tab.
2. Spotifys "Deine Bibliothek" ist das Vorbild: **eine** Liste mit Filter-Chips, Sortierung (pro Filter gemerkt), Raster/Liste-Umschalter und Anheften.
3. Die größte Spotify-Kritik ist ein **überladenes, feed-artiges Home** und eine undurchsichtige Warteschlange (Smart Shuffle, "Warteschlange leeren" nur manchmal sichtbar).
4. Bei Subsonic-Clients gewinnt **Symfonium** (Tiefe, Anpassbarkeit, Offline-Cache, schnelle Sync), verliert aber bei Einstiegshürde und Preis; **Tempo** wird für Optik und große Bibliotheken gelobt; **Substreamer** gilt als "veraltet wirkend", mit Wiedergabe-Aussetzern und Paywall bei Chromecast.
5. Häufigste Client-Probleme: Ruckeln/Einfrieren bei großen Bibliotheken, Download-Anzeigen die nicht aktualisieren, Offline-Modus mit falschen Daten, Wiedergabe die nach Netzverlust stoppt.
6. Nocturne gewinnt, wenn es **Spotify-Einfachheit** (3–4 Tabs, klare Warteschlange, Swipe-to-Queue) mit **Symfonium-Robustheit** (lokale DB, schneller Sync, transparenter Offline-Zustand) verbindet.

---

## 1. Spotify (Android)

### 1.1 Tab-Leiste
- **[Beleg]** Jahrelang drei Tabs: Home, Suche, Bibliothek; 2025 kam "Create" als vierter hinzu, Bibliothek rutschte an Platz 3 ([Android Authority, Create-Tab](https://androidauthority.com/spotify-create-tab-inconvenience-3562796); [TechCrunch, Mai 2025](https://techcrunch.com/2025/05/07/spotifys-latest-update-gives-users-more-control-over-their-listening-experience)).
- **[Beleg]** Der Create-Button wurde nach Nutzerprotest ab August 2025 abschaltbar ([Android Authority](https://www.androidauthority.com/spotify-create-button-toggle-3583374/); [Spotify Community, Idee "Get rid of Create button"](https://community.spotify.com/t5/Implemented-Ideas/Mobile-Get-rid-of-Create-button-on-mobile-navigation-bar/idi-p/6453037/page/3)).
- **[Beleg]** Früher gab es mehr Tabs (Browse, Radio); diese wurden in die Suche verschoben ([TechRadar](https://techradar.com/news/spotify-cuts-the-fat-from-its-premium-app-with-streamlined-overhaul); [PhoneArena](https://www.phonearena.com/news/Spotify-for-Android-finally-puts-the-navigation-bar-where-it-belongs---at-the-bottom-of-your-screen_id88111)).
- **[Einschätzung]** Lehre: Nutzer bestrafen Tabs, die nicht täglich gebraucht werden. Seltene Funktionen gehören ins Profil oder in Kontextmenüs.

### 1.2 Home
- **[Beleg]** Home hat Filter-Chips oben ("Alle", "Musik", "Podcasts", ...) ([TechCrunch 2022](https://techcrunch.com/2022/08/09/spotify-updates-home-screen-with-new-personalized-discovery-feeds-music-podcasts); [Android Central](https://androidcentral.com/apps-software/spotify-new-home-feeds-for-music-and-podcasts)).
- **[Beleg] Kritik:** Das Feed-artige Home von 2023 wurde als "TikTok-Kopie" kritisiert ([inkl/News](https://www.inkl.com/news/spotify-update-leaves-users-fuming-as-they-accuse-app-of-trying-to-be-tiktok)); Nutzer beklagen ein überladenes, unübersichtliches UI ([Spotify Community: "UI Needs a Redesign – Cluttered, Messy"](https://community.spotify.com/t5/Social-Random/Spotify-s-UI-Needs-a-Redesign-It-s-Cluttered-Messy-and/m-p/7021650/highlight/true); [Thread "What's up with this new Home screen"](https://community.spotify.com/t5/iOS-iPhone-iPad/What-s-up-with-this-new-Home-screen/td-p/5435858/page/50)); Nutzer berichteten, dass gewohnte Mixe plötzlich nicht mehr auffindbar waren ([Spotify Community](https://community.spotify.com/t5/App-Features/Spotify-App-Home-Page-Redesign-Update/td-p/5462703)).
- **[Einschätzung]** Für einen Client der eigenen Bibliothek ist das Problem kleiner, weil keine Werbe-/Entdeckungsinteressen bestehen. Home sollte aber **stabil** sein (gleiche Reihenfolge der Abschnitte), sonst entsteht dieselbe Kritik.

### 1.3 Suche
- **[Beleg]** Suche = Suchfeld + Kategorie-Kacheln (Genres); beim Tippen ins Feld erscheinen "Zuletzt gesucht" ([TechRadar](https://techradar.com/uk/news/spotify-cuts-the-fat-from-its-premium-app-with-streamlined-overhaul)); später kamen Filter-Chips (Musik/Podcasts) dazu ([Engadget](https://engadget.com/spotify-search-filters-music-podcasts-ios-android-162824831.html)).
- **[Beleg] Kritik:** "Browse All" ohne erkennbare Sortierung ([UX-Case-Study, uxdesign.cc Bootcamp](https://bootcamp.uxdesign.cc/case-study-follow-up-to-spotify-ui-updates-1aa018ed3e23)); ein Tipp auf einen Eintrag in "Zuletzt gesucht" spielt den Song sofort statt die Suche zu wiederholen ([Spotify Community](https://community.spotify.com/t5/Android/Recent-searches-beaviour-broken/m-p/5245732)).

### 1.4 Deine Bibliothek
- **[Beleg]** Redesign 2021: Filter-Chips (Playlists, Alben, Künstler, Heruntergeladen …), ohne Filter = alles gemischt; Sortierung "Zuletzt gehört / Zuletzt hinzugefügt / Alphabetisch / Ersteller"; Umschalter Raster/Liste; Anheften per langem Druck oder Wischen nach rechts ([MacRumors](https://www.macrumors.com/2021/04/29/spotify-updated-your-library-filters-grid-view/); [Android Central](https://www.androidcentral.com/spotify-your-library-ui-update-dynamic-filters); [Spotify Community, Ankündigung](https://community.spotify.com/t5/Your-Library/New-Your-Library-view-iOS-amp-Android/m-p/5203031/highlight/true)).
- **[Beleg]** Die Sortierung wird pro Filter gemerkt (ab Okt. 2021) ([Spotify Community](https://community.spotify.com/t5/Your-Library/New-Your-Library-view-iOS-amp-Android/m-p/5214453/highlight/true)); Anheften ist auf 4 Elemente begrenzt ([SlashGear](https://www.slashgear.com/spotify-updates-your-library-with-pinned-playlists-and-smarter-search-29670970)).
- **[Beleg]** Seit 2025 oben in Playlists: Hinzufügen / Bearbeiten / Sortieren ([TechCrunch](https://techcrunch.com/2025/05/07/spotifys-latest-update-gives-users-more-control-over-their-listening-experience)).

### 1.5 Now Playing und Warteschlange
- **[Beleg]** Warteschlangen-Button wurde 2019 nach unten rechts verlegt, damit er einhändig erreichbar ist ([Android Police](https://www.androidpolice.com/2019/03/15/spotify-makes-your-song-queue-more-accessible-with-new-now-playing-ui/)); zuvor in ein Menü versteckte Repeat-/Queue-Buttons kamen nach Protest zurück ([Spotify Community](https://community.spotify.com/t5/Implemented-Ideas/Mobile-Return-repeat-and-queue-buttons-to-Now-Playing-screen/idc-p/4689545/highlight/true)).
- **[Beleg]** 2025: In der Warteschlange sind Shuffle, Smart Shuffle, Repeat und Sleep-Timer direkt erreichbar; empfohlene Titel nach der eigenen Warteschlange werden angezeigt ([TechCrunch](https://techcrunch.com/2025/05/07/spotifys-latest-update-gives-users-more-control-over-their-listening-experience)).
- **[Beleg] Kritik:** Liedtext-Position unter dem Cover/Scrollen wurde kritisiert, Spotify stellte das Lyrics-Icon wieder her ([Spotify Community](https://community.spotify.com/t5/Implemented-Ideas/Change-lyrics-position/idc-p/7039393/highlight/true)).
- **[Beleg] Kritik Warteschlange:** "Warteschlange leeren" erscheint nur bei manuell hinzugefügten Titeln ([Spotify Community](https://community.spotify.com/t5/Content-Questions/Bug-on-clear-play-queue-button/td-p/7005965)); Smart Shuffle lädt die Warteschlange beim Hinzufügen neu bzw. lässt sich schwer verlassen ([Spotify Community](https://community.spotify.com/t5/Your-Library/Smart-shuffle-breaks-if-you-interact-with-it/td-p/6119113); [Queue re-shuffles](https://community.spotify.com/t5/Your-Library/Queue-re-shuffles-when-adding-songs-to-playlist/td-p/7301880)); "Aus Warteschlange entfernen" defekt ([Spotify Community](https://community.spotify.com/t5/iOS-iPhone-iPad/Several-issues-with-queue-and-shuffling/td-p/6138664)).
- **[Vergleich, Beleg]** YouTube Music hat 2026 die Warteschlange ("Up Next") als halbhohes Panel per Wisch nach oben auf dem Player umgesetzt ([Android Authority](https://www.androidauthority.com/youtube-music-now-playing-redesign-split-screen-3657648/); [9to5Google](https://9to5google.com/2026/04/23/youtube-music-split-now-playing-redesign/)).

### 1.6 Gesten und Kontextmenüs
- **[Beleg]** Android: Wisch rechts auf Titel = zur Warteschlange; Wisch links in der Warteschlange = entfernen; Wisch hoch/runter öffnet/schließt Now Playing; Wisch links/rechts auf dem Mini-Player = nächster/vorheriger Titel; langes Drücken = Kontextmenü ([Android Authority](https://www.androidauthority.com/spotify-shortcuts-gestures-android-3644492/)).
- **[Beleg]** Swipe-to-Queue gab es auf iOS seit 2015, auf Android erst Jahre später nach anhaltenden Nutzerwünschen ([PhoneArena](https://www.phonearena.com/news/spotify-swipe-to-queue-android-update_id138826); [Spotify Community Idee](https://community.spotify.com/t5/Implemented-Ideas/Android-Swipe-to-Queue-a-Track/idi-p/1189608/page/29)).
- **[Beleg] Kritik:** Die Android-Zurück-Geste wird am Mini-Player teils als Skip-Wisch erkannt ([Android Authority](https://www.androidauthority.com/spotify-shortcuts-gestures-android-3644492/)).

### 1.7 Profil und Statistik
- **[Beleg]** Seit 6. Nov. 2025: "Listening stats" erreichbar über Tipp aufs Profilbild; zeigt Top-Künstler und Top-Songs der letzten 4 Wochen, Highlights (Meilensteine, Entdeckungen), teilbar ([Spotify Newsroom](https://newsroom.spotify.com/2025-11-06/spotify-new-feature-listening-stats/); [9to5Google](https://9to5google.com/2025/11/06/spotify-listening-stats-rolling-out/); [TechCrunch](https://techcrunch.com/2025/11/06/spotify-now-lets-you-see-weekly-listening-stats/)).
- **[Beleg]** Laut Spotify-Community liegt auch "Einstellungen" hinter dem Profilbild (Profil → Einstellungen → Inhalte und Anzeige) ([Spotify Community](https://community.spotify.com/t5/Implemented-Ideas/Mobile-Get-rid-of-Create-button-on-mobile-navigation-bar/idc-p/7048640/highlight/true)) – nutzerberichtet.

---

## 2. Subsonic-/Navidrome-/Jellyfin-Clients

### 2.1 Symfonium (Android, kostenpflichtig)
- **Aufbau [Beleg]:** Hauptnavigation frei wählbar und sortierbar; entfernt man "Einstellungen" aus der Leiste, wandert es in die obere Leiste. Home besteht aus ein-/ausblendbaren, per Drag sortierbaren Zeilen und Shortcuts ([Symfonium Docs, Interface](https://docs.symfonium.app/wiki/other/symfonium-configurable-interface/); [Docs, Home page](https://docs.symfonium.app/wiki/settings/settings-interface-home-page/); [Docs, Navigation ausblenden](https://docs.symfonium.app/tips-and-tricks/hide-the-navigation-bar/)).
- **Offline [Beleg]:** Automatischer Offline-Cache für ganzen Server oder einzelne Bibliotheken, permanenter Cache plus größenbegrenzter "Rolling Cache" ([Symfonium Docs, Offline-Cache](https://docs.symfonium.app/wiki/other/offline-media-cache-downloads-rolling-cache-playback-cache-automatic-cache-and-more/)).
- **Große Bibliotheken [Beleg]:** Lokale Datenbank-Sync; "Fast Sync" nur mit OpenSubsonic-Servern, alte Server werden bei großen Bibliotheken ausdrücklich abgeraten ([Symfonium Docs, Subsonic-Provider](https://docs.symfonium.app/wiki/providers/subsonic-opensubsonic-media-provider-configuration/)); Entwickler nennt ca. 2 Minuten für 60k Titel (Herstellerangabe) ([Support-Forum](https://support.symfonium.app/t/assistance-with-symfonium-playlist-management-and-syncing-of-large-music-library/5481)).
- **Lob [Beleg]:** "endless customization", leicht und flott, guter Support ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)); in Communities am häufigsten als bester Android-Client genannt ([Lemmy-Thread](https://deddit.petersanchez.com/g/selfhosted@lemmy.world/p/5Wg2zfff33xNb4z329-Best-Free-Mobile-App-for-Streaming-SelfHosted-Mu)); Gapless, ReplayGain, Android Auto, Chromecast ([Unstore-Test](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)).
- **Kritik [Beleg]:** Dichte, funktionsüberladene Oberfläche mit Einarbeitungszeit; kostenpflichtig ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)); Closed Source ([Lemmy-Thread](https://deddit.petersanchez.com/g/selfhosted@lemmy.world/p/5Wg2zfff33xNb4z329-Best-Free-Mobile-App-for-Streaming-SelfHosted-Mu)).

### 2.2 Substreamer (Android/iOS)
- **Aufbau [Beleg]:** Album-Art, Playlists und Suche im Vordergrund, freundlich; Offline-Cache pro Titel/Album ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)). Laut Store-Text: Downloads von Alben/Playlists mit Hintergrund-Warteschlange, automatischer Offline-Modus wenn der Server nicht erreichbar ist ([App Store](https://apps.apple.com/us/app/id1012991665?l=en-us)).
- **Kritik [Beleg]:**
  - Konventionen wirken "dated" im Vergleich zu Tempo/Symfonium ([Unstore, Symfonium-Alternativen](https://unstore.io/discover/best-symfonium-alternatives/)).
  - Chromecast nur in Premium, Werbung in der Gratisversion; verzögerte Bibliotheks-Aktualisierung nach großen Server-Änderungen ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)).
  - Kurze Wiedergabe-Aussetzer ("nano-interruptions") veranlassten einen Nutzer zum Wechsel zu Tempo; ein anderer nennt die App "kinda a mess" ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)).
  - Warteschlange ließ sich nicht leeren, nur ersetzen; Mediensteuerung blieb auf dem Sperrbildschirm (ältere iOS-Rezension; Entwickler verweist auf Neuentwicklung) ([App Store](https://apps.apple.com/gb/app/substreamer/id1012991665)).
  - Kein Android Auto (Blog-Angabe, nicht verifiziert) ([JenAI-Blog](https://jenaichat.com/blog/navidrome-android-app-clients/)).
- **[Einschätzung]** Play-Store-Rezensionen ließen sich nicht abrufen; die Kritik basiert daher vor allem auf Foren/Tests.

### 2.3 Tempo / Tempus (Android, Open Source)
- **Aufbau [Beleg]:** Material You, Jetpack Compose, cover-lastiges Browsing mit Spotify-Anmutung; Smart Shuffle über die ganze Bibliothek, Offline-Cache pro Album ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/); [GitHub](https://github.com/CappielloAntonio/tempo)).
- **Lob [Beleg]:** "slick UI" ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)); einziger Client, der bei großer Bibliothek nicht "erstickt" (Nutzerkommentar) ([Lemmy](https://lemmy.amethyst.name/post/1473403/618530)).
- **Kritik [Beleg]:** Absturz/Startproblem bis zum Zurücksetzen ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)); Randfälle wie Mehr-CD-Alben ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)); Upstream zeitweise unmaintained, Fork "Tempus" mit Performance-Arbeit für große Bibliotheken ([Tempus-Release-Thread](https://deddit.petersanchez.com/g/selfhosted@lemmy.world/p/gSG24Jyx2lhsb7P5By-Tempus-v413-android-subsonic-client-release)).

### 2.4 Ultrasonic und DSub (Android, Open Source)
- **[Beleg]** Ultrasonic: minimal, klein, keine Telemetrie, kein Chromecast ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)); Material-You-Theme, begrenzte Entwicklerkapazität ([GitHub](https://github.com/ultrasonic/ultrasonic); [F-Droid](https://f-droid.org/en/packages/org.moire.ultrasonic/)).
- **[Beleg]** DSub: sehr ausgereift, aber Optik "wie 2013"; langsamere Entwicklung ([Unstore](https://unstore.io/discover/best-subsonic-self-hosted-music-client-apps-android/)); weniger Anpassung als Symfonium ([Lemmy-Thread](https://deddit.petersanchez.com/g/selfhosted@lemmy.world/p/5Wg2zfff33xNb4z329-Best-Free-Mobile-App-for-Streaming-SelfHosted-Mu)).

### 2.5 Finamp (Jellyfin, Flutter – technisch Nocturne am nächsten)
- **Aufbau [Beleg]:** Redesign mit neuem Player, Mini-Player, Warteschlangen-Panel und Song-Menü; Warteschlange in vier Abschnitten: vorherige Titel, aktueller Titel, "Next Up", reguläre Warteschlange; Hinzufügen zu "Next Up" am Anfang oder Ende ([Finamp 0.9.2-beta](https://www.github.com/jmshrv/finamp/releases/tag/0.9.2-beta)); Radio-Funktion füllt nach Ende der Warteschlange ähnliche Titel nach ([0.9.21-beta](https://newreleases.io/project/github/finamp-app/finamp/release/0.9.21-beta)); 1.0 bringt Android Auto/CarPlay ([1.0.1](https://newreleases.io/project/github/finamp-app/finamp/release/1.0.1)).
- **Kritik [Beleg]:** Neuordnen der Warteschlange bei aktivem Shuffle zeitweise nicht möglich; Wiedergabe stoppt nach OS-Suspend ([Releases](https://www.github.com/jmshrv/finamp/releases)); Löschen großer Elemente friert die App ein, Download-Anzeigen aktualisieren nicht ([F-Droid](https://f-droid.org/en/packages/com.unicornsonlsd.finamp/)); Out-of-Memory bei großen Downloads ([0.9.19-beta](https://newreleases.io/project/github/finamp-app/finamp/release/0.9.19-beta)); Offline-Modus vermischte Bibliotheken, Offline-Favoriten spielten falsche Titel ([Issues](https://www.Github.com/jmshrv/finamp/issues)).

### 2.6 Feishin (Desktop), Amperfy, Arpeggi (iOS)
- **[Beleg] Feishin:** Spotify-inspirierte Oberfläche, GUI-Editor für Smart-Playlists gelobt; speicherhungrig ([LinuxLinks](https://www.linuxlinks.com/feishin-modern-self-hosted-music-player/2/); [Lemmy](https://lemmy.mariusdavid.fr/post/145371)).
- **[Beleg] Amperfy:** Offline-Modus, CarPlay, Sleep-Timer, EQ; Rezensionen nennen sowohl "polished" als auch Ruckler/Abstürze; Cache lädt standardmäßig jeden gespielten Song ([App Store](https://apps.apple.com/us/app/amperfy-music/id1530145038)); "etwas zu minimal" ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)).
- **[Beleg] Arpeggi:** Sehr gelobt, aber "Alle zufällig" spielte nur die ersten n Songs ([Navidrome Discussion #2375](https://github.com/navidrome/navidrome/discussions/2375)).
- **[Beleg] Navic:** Beste UI/UX laut einem Nutzer, aber instabil bei großen Bibliotheken ([Lemmy](https://lemmy.amethyst.name/post/1473403/618530)).

---

## 3. Allgemeine Best Practices (Android)

- **Tab-Leiste [Beleg]:** 3–5 Ziele gleicher Hierarchie; bei 3 Zielen Icon + Label für alle; ab großen Bildschirmen Navigation Rail ([Android Layout & Nav Patterns](https://developer.android.google.cn/design/ui/mobile/guides/layout-and-content/layout-and-nav-patterns?hl=en); [Compose Navigation Bar](https://developer.android.google.cn/develop/ui/compose/components/navigation-bar?hl=ca)). Navigation Drawer ist schlechter, weil oben schwer erreichbar (ebd.).
- **Touch-Ziele & Kontrast [Beleg]:** mind. 48×48 dp, Abstand ≥ 8 dp; Textkontrast ≥ 4,5:1, Icons/Flächen ≥ 3:1 ([Android Accessibility Foundations](https://developer.android.com/design/ui/mobile/guides/foundations/accessibility); [Google Support, Touch target size](https://support.google.com/accessibility/android/answer/7101858?hl=en)).
- **Einhand-Bedienung [Beleg]:** Spotify verlegte Queue nach unten rechts für Erreichbarkeit ([Android Police](https://www.androidpolice.com/2019/03/15/spotify-makes-your-song-queue-more-accessible-with-new-now-playing-ui/)).
- **Auto [Beleg]:** Browse-Inhalte maximal etwa 3 Ebenen tief, Liste als Standard, Unterkategorien mit Überschrift, Suche dringend empfohlen, max. 4 Tabs mit Label + monochromem Icon, keine zu breiten Kategorien ([Android for Cars: Browsing Views](https://developer.android.com/design/ui/cars/guides/app-types/plan-browsing-views); [Navigation Tabs](https://developer.android.google.cn/design/ui/cars/guides/app-types/plan-navigation-tabs)).
- **Lange Listen in Flutter [Beleg]:** Lazy Builder (`ListView.builder`), feste Zeilenhöhe via `itemExtent`/`prototypeItem`, kein `shrinkWrap`, keine verschachtelten ListViews ([Flutter Performance Best Practices](https://docs.flutter.dev/perf/best-practices.html); [LogRocket](https://blog.logrocket.com/performance-improvements-mobile-apps-flutter/); [Medium: Common mistakes with ListViews](https://medium.com/@pomis172/common-mistakes-with-listviews-in-flutter-f22e7dacfaf7)).

---

## Empfehlungen für Nocturne

Jede Empfehlung: **Was** – **Warum** (Quelle aus den Abschnitten oben). Wo nichts anderes steht, ist die konkrete Ausgestaltung **[Einschätzung]**, die Begründung belegt.

### MUSS

**M1 – Tab-Leiste auf 3 Tabs: Home · Suche · Bibliothek.**
- Statistik-Tab entfällt → wandert ins Profil (M2).
- Downloads-Tab entfällt → wird Filter-Chip "Heruntergeladen" in der Bibliothek plus Eintrag "Downloads verwalten" im Profil-Menü (Fortschritt, Speicherplatz, Fehler).
- Einstellungen → Profil-Menü.
- Labels immer sichtbar (bei 3 Zielen empfohlen), Tab-Höhe ≥ 48 dp Ziel.
- Erneuter Tipp auf aktiven Tab = nach oben scrollen bzw. zur Wurzel zurück.
- *Warum:* Spotify fährt seit Jahren 3 Tabs und musste den 4. abschaltbar machen (1.1); Android empfiehlt 3–5 Ziele gleicher Ebene (3); Android Auto erlaubt max. 4 Tabs (3).

**M2 – Profil-Avatar oben links auf Home, Suche und Bibliothek öffnet ein Profil-Sheet/-Seite.**
- Inhalt in dieser Reihenfolge: Kopf (Server-Name, Benutzer, Online/Offline-Status) · "Deine Statistik" · "Downloads verwalten" · "Equalizer" · "Einstellungen" · "Server wechseln / Konto" · "Über".
- "Deine Statistik" öffnet die bestehende Statistikseite; Standard-Zeitraum "Letzte 4 Wochen", oben Top-Künstler und Top-Titel, darunter Hörzeit, Tageszeiten, Aktivitätsraster; Teilen-Button (als Bild).
- *Warum:* Spotify platziert Listening Stats exakt so (Profilbild → Stats, 4 Wochen, Top-Künstler/-Titel, teilbar) (1.7); ausdrücklicher Nutzerwunsch.

**M3 – Bibliothek als EINE Liste im Spotify-Modell.**
- Filter-Chips horizontal: Playlists · Alben · Künstler · Heruntergeladen · Favoriten · Smart. Kein Chip aktiv = gemischte Liste aller Sammlungen. Ein Chip aktiv → weitere Chips ausblenden und "×" zum Zurücksetzen zeigen.
- Zeile darunter: links Sortier-Button (Zuletzt gehört / Zuletzt hinzugefügt / Alphabetisch / Künstler), rechts Raster-/Liste-Umschalter. **Sortierung und Ansicht pro Chip persistent speichern.**
- Oben angeheftet: "Lieblingssongs" (Favoriten) und bis zu 4 vom Nutzer angeheftete Elemente (langes Drücken → "Anheften").
- Suchfeld in der Bibliothek (Lupe in der App-Bar), filtert lokal.
- *Warum:* Spotify-Bibliothek ist der etablierte Standard; Sortierung pro Filter wurde nachgerüstet, weil Nutzer sie erwarteten (1.4).

**M4 – Warteschlange transparent und vollständig steuerbar.**
- Abschnitte: "Läuft gerade" · "Als Nächstes in der Warteschlange" (manuell) · "Als Nächstes von: <Quelle>" · optional "Zuvor gespielt" (eingeklappt).
- "Warteschlange leeren" **immer** sichtbar, wenn der manuelle Abschnitt nicht leer ist; zusätzlich "Quelle beenden" (leert auch den Quellenabschnitt).
- Ziehgriff rechts (48 dp) zum Neuordnen – **auch bei aktivem Shuffle** (Shuffle = Reihenfolge wird beim Einschalten neu gewürfelt, danach frei sortierbar).
- Wisch links = entfernen, mit Snackbar "Rückgängig".
- Shuffle, Wiederholen, Sleep-Timer zusätzlich in der Kopfzeile der Warteschlange.
- Hinzufügen nie die Warteschlange neu mischen lassen.
- *Warum:* Spotify-Kritik zu verstecktem Leeren-Button, Neu-Mischen und kaputtem Entfernen (1.5); Substreamer konnte die Warteschlange nicht leeren (2.2); Finamp konnte bei Shuffle nicht neu ordnen (2.5); Spotify stellte Shuffle/Repeat/Sleep-Timer 2025 in die Queue (1.5).

**M5 – Swipe-to-Queue als Standard-Geste auf allen Titelzeilen.**
- Standard: kurz rechts = "Zur Warteschlange hinzufügen" (ans Ende von "Als Nächstes in der Warteschlange"), lang rechts = "Als Nächstes spielen", kurz links = Favorit an/aus. Bestehende Konfigurierbarkeit beibehalten, aber mit diesen Defaults.
- Haptisches Feedback + Snackbar "Zur Warteschlange hinzugefügt · Rückgängig".
- In der Warteschlange selbst: links = entfernen.
- *Warum:* Swipe-to-Queue ist Spotifys meistgewünschte Geste auf Android (1.6); Finamp unterscheidet "Next Up" vorne/hinten (2.5).

**M6 – Mini-Player: Gesten ohne Konflikt mit System-Zurück.**
- Tipp oder Wisch hoch = Vollbild-Player; horizontale Wische = nächster/vorheriger Titel **nur innerhalb des Cover-/Titelbereichs** mit Mindestdistanz (~1/3 Breite) und nicht in den äußeren 24 dp (System-Gestenzone, via `MediaQuery.systemGestureInsets`).
- Play/Pause-Button rechts mind. 48×48 dp; dünne Fortschrittslinie am unteren Rand.
- *Warum:* Spotify-Gesten (1.6) und dokumentierter Fehlauslöser durch die Zurück-Geste (1.6).

**M7 – Now-Playing einhändig bedienbar.**
- Alle häufigen Steuerungen in der unteren Bildschirmhälfte: Zeile 1 Titel/Künstler + Favorit-Herz; Zeile 2 Fortschritt mit Zeitangaben; Zeile 3 Shuffle · Zurück · Play (≥ 64 dp) · Weiter · Wiederholen; Zeile 4 links Ausgabegerät/Qualität, rechts Songtext · Warteschlange.
- Wisch hoch auf dem Player öffnet die Warteschlange als halbhohes Sheet (vergrößerbar); Wisch runter schließt den Player.
- Songtext als eigener Button (nicht nur durch Scrollen erreichbar).
- Sleep-Timer und "Zum Album/Künstler" im Drei-Punkte-Menü oben rechts.
- *Warum:* Spotify verlegte Queue nach unten für Einhandbetrieb und musste versteckte Buttons zurückbringen (1.5); Kritik an Lyrics-Position (1.5); YouTube-Music-Queue-Sheet (1.5).

**M8 – Lokale Bibliotheks-Datenbank mit inkrementellem Sync; Listen virtualisiert.**
- Alle Listen per `ListView.builder`/`SliverList` mit festem `itemExtent` (z. B. 64 dp Zeilen), kein `shrinkWrap`, keine verschachtelten ListViews; Cover-Thumbnails in Zeilengröße laden und cachen.
- Künstler/Alben-Listen ab ~200 Einträgen mit A–Z-Schnellscroll-Leiste.
- Sync im Hintergrund mit sichtbarem Status im Profil ("Bibliothek aktualisiert vor 5 Min."), manuelles "Jetzt synchronisieren"; OpenSubsonic-Erweiterungen nutzen, wenn verfügbar.
- *Warum:* Ruckeln/Abstürze bei großen Bibliotheken sind der häufigste Kritikpunkt (Navic, Finamp, Tempo-Fork) (2.3, 2.5, 2.6); Symfonium setzt auf lokale DB + Fast Sync (2.1); Substreamer: verzögerte Aktualisierung (2.2); Flutter-Best-Practices (3).

**M9 – Offline-Zustand immer ehrlich anzeigen.**
- Globaler Offline-Banner (unter der App-Bar, dezent) wenn Server nicht erreichbar; dann alle nicht heruntergeladenen Titel ausgegraut (50 % Deckkraft) aber sichtbar, Tipp darauf → Snackbar "Nicht offline verfügbar".
- Download-Symbol (Haken/Kreis mit Fortschritt) an jeder Zeile, Album und Playlist; Fortschrittsanzeige live aktualisiert.
- Wiedergabe bei Netzwechsel automatisch fortsetzen (Retry mit Backoff), nicht still stoppen.
- *Warum:* Finamp: Download-Anzeigen aktualisieren nicht, Offline-Daten falsch (2.5); Navidrome-Bug "Queue stops after network loss" und Substreamer-Aussetzer (2.2); Substreamer schaltet automatisch in Offline-Modus (2.2).

**M10 – Kontextmenü als Bottom-Sheet, identisch für Drei-Punkte und langes Drücken.**
- Kopf: Cover + Titel + Künstler. Einträge in fester Reihenfolge: Als Nächstes spielen · Zur Warteschlange · Zu Playlist hinzufügen · Favorit · Herunterladen/Download entfernen · Zum Album · Zum Künstler · Songtext · Info (Format, Bitrate, Dateigröße) · Teilen.
- Für Alben/Playlists analog (Abspielen, Zufällig, Herunterladen, Anheften, Bearbeiten/Löschen nur bei eigenen Playlists).
- Jede Zeile ≥ 48 dp hoch.
- *Warum:* Spotify: langes Drücken = Kontextmenü (1.6); Touch-Ziel-Richtlinie (3). [Einschätzung: gleiche Reihenfolge überall senkt Fehlbedienung.]

### SOLLTE

**S1 – Home stabil und eigenbibliotheksbezogen, keine Feed-Logik.**
- Reihenfolge: Begrüßung + Profil-Avatar · Filter-Chips (Alle / Alben / Playlists / Heruntergeladen) · 2×4-Raster "Zuletzt gehört" (Kacheln mit Cover + Name, wie Spotify-Shortcuts) · "Kürzlich hinzugefügt" (horizontale Karussell-Reihe) · "Deine Top-Alben dieses Monats" · "Zufällige Alben" (mit Neu-würfeln-Button) · "Smart-Playlists" · "Wiederentdecken" (lange nicht gehört).
- Reihen ein-/ausblendbar und sortierbar in Einstellungen → Home.
- *Warum:* Kritik am überladenen, wechselhaften Spotify-Home (1.2); Symfonium-Home mit konfigurierbaren Reihen ist gelobte Anpassbarkeit (2.1).

**S2 – Suche: sofortige lokale Ergebnisse, gruppiert.**
- Ohne Eingabe: "Zuletzt gesucht" (Tipp = **Suche erneut ausführen bzw. Element öffnen**, nicht sofort abspielen; × zum Entfernen) und darunter Genre-Kacheln aus der Bibliothek.
- Mit Eingabe: Chips Alle / Titel / Alben / Künstler / Playlists; "Top-Ergebnis" groß oben; Debounce ~250 ms; Ergebnisse aus lokaler DB, Server-Suche als Ergänzung.
- *Warum:* Spotify-Aufbau (1.3); Kritik an Recent-Search-Verhalten und unsortiertem "Browse All" (1.3); Android Auto empfiehlt Suche (3).

**S3 – Leerzustände mit klarer nächster Aktion.**
- Bibliothek leer/Sync läuft: Skeleton-Zeilen + "Bibliothek wird synchronisiert … 1 240 / 8 300 Titel".
- Heruntergeladen leer: Icon + "Noch nichts offline" + Button "Album herunterladen" → Bibliothek/Alben.
- Warteschlange leer: "Nichts in der Warteschlange" + Buttons "Zufallsmix starten" · "Zur Bibliothek".
- Suche ohne Treffer: "Keine Treffer für »xyz«" + Hinweis "Offline werden nur heruntergeladene Titel durchsucht" (falls offline).
- Statistik ohne Daten: "Hör ein paar Songs – deine Statistik erscheint hier."
- *Warum:* [Einschätzung] Keine Quelle direkt; abgeleitet aus Kritik an unklarem Offline- und Sync-Verhalten (2.2, 2.5).

**S4 – Fehlermeldungen konkret und handlungsorientiert.**
- Format: Was ist passiert + was tun. Beispiele: "Server nicht erreichbar (Zeitüberschreitung). Offline-Titel bleiben verfügbar. [Erneut versuchen]"; "Anmeldung fehlgeschlagen: Benutzername oder Passwort falsch."; "Server unterstützt kein Fast Sync – erster Abgleich kann länger dauern."; Download-Fehler pro Titel im Downloads-Bereich mit "Erneut versuchen"/"Alle erneut versuchen".
- Keine Rohfehler (HTTP-Codes nur in "Details").
- *Warum:* Symfonium warnt ausdrücklich vor langsamen alten Servern (2.1); Substreamer-Support musste Proxy-/Encoding-Probleme manuell diagnostizieren ([App Store](https://apps.apple.com/us/app/id1012991665?l=en-us)).

**S5 – Onboarding/Login in maximal 3 Schritten.**
- Schritt 1: Server-Typ automatisch erkennen (URL eingeben, App probiert Subsonic/OpenSubsonic/Jellyfin), Feld mit Beispiel "https://musik.example.org".
- Schritt 2: Benutzer + Passwort, Button "Verbinden" mit Verbindungstest; Fehler direkt am Feld.
- Schritt 3: "Bibliothek wird geladen" mit Fortschritt; App ist sofort nutzbar (Home mit Skeletons), Sync läuft weiter.
- Optional danach: einmaliger Hinweis-Coach "Wische nach rechts, um zur Warteschlange hinzuzufügen".
- *Warum:* Symfonium-Kritik "steile Lernkurve" (2.1); Substreamer/Navidrome-Probleme mit Proxy/Encoding (2.2). Ausgestaltung [Einschätzung].

**S6 – Android Auto mit max. 4 Tabs, ≤ 3 Ebenen.**
- Tabs: Zuletzt gehört · Playlists · Alben · Heruntergeladen; Listen als Standard, Suche aktiv.
- *Warum:* Android-for-Cars-Richtlinien (3); Fehlen von Android Auto wird bei Substreamer genannt (2.2); Finamp führte es mit 1.0 ein (2.5).

**S7 – "Zufällig alle" wirklich über die ganze Bibliothek.**
- Serverseitig `getRandomSongs` in Blöcken nachladen statt nur die ersten n Titel.
- *Warum:* Arpeggi-Kritik (2.6); Tempo-Smart-Shuffle über ganze Bibliothek gelobt (2.3); Finamp musste Shuffle tausender Titel begrenzen (2.5).

**S8 – Barrierefreiheit als Abnahmekriterium.**
- Alle Icons mit `Semantics`-Label, Kontrast ≥ 4,5:1 Text / 3:1 Icons auch auf dynamischen Cover-Farben (Hintergrund ggf. abdunkeln), Schrift skalierbar bis 200 % ohne abgeschnittene Steuerung, Gesten immer auch per Menü erreichbar.
- *Warum:* Android-Accessibility-Richtlinien (3).

### KANN

**K1 – Radio/Autoplay nach Ende der Warteschlange** (ähnliche Titel desselben Künstlers/Genres), abschaltbar; anzeigen als eigener Abschnitt "Danach: Ähnliche Titel". *Warum:* Finamp-Radio (2.5); Spotify zeigt Folgetitel nach der Queue an und machte Autoplay abschaltbar (1.5).

**K2 – Wöchentliche Statistik-Highlights** ("Neuer Meilenstein: 1 000 Stunden", "Neu entdeckt: <Künstler>") als Karte auf Home, wegklickbar. *Warum:* Spotify Listening Stats Highlights (1.7).

**K3 – Konfigurierbare Haupt-Tabs** (z. B. Downloads statt Suche) für Power-User, Default bleibt 3 Tabs. *Warum:* Symfonium-Anpassbarkeit (2.1), aber Dichte-Kritik → nur optional.

**K4 – Dynamische Farbe aus dem Cover** im Vollbild-Player (gedämpft, Kontrast geprüft). *Warum:* Plexamp-Farbextraktion wird gelobt ([iMore](https://imore.com/plexamp-music-player-plex-macos-all-bells-and-whistles)).

**K5 – Navigation Rail auf Tablets/Querformat.** *Warum:* Android-Richtlinie (3).

---

## Häufige Fehler anderer Apps, die Nocturne vermeiden soll

1. **Tabs für Nebenfunktionen** – Spotifys Create-Tab musste abschaltbar werden (1.1). → Keine Tabs für Statistik, Downloads, Einstellungen.
2. **Home, das sich ständig umbaut** – Nutzer fanden gewohnte Inhalte nicht mehr (1.2). → Feste Reihen, nur vom Nutzer änderbar.
3. **Versteckte oder inkonsistente Warteschlangen-Aktionen** – Leeren-Button nur manchmal (Spotify), gar nicht (Substreamer), Neuordnen bei Shuffle unmöglich (Finamp) (1.5, 2.2, 2.5).
4. **Warteschlange, die sich beim Hinzufügen neu mischt** (Spotify Smart Shuffle) (1.5).
5. **Wichtige Buttons in Menüs verstecken** – Spotify musste Repeat/Queue zurückbringen (1.5).
6. **Gesten, die mit System-Gesten kollidieren** – Zurück-Geste löst Skip aus (1.6).
7. **Ruckeln/Einfrieren bei großen Bibliotheken** und bei Löschen großer Elemente (Navic, Finamp) (2.5, 2.6).
8. **Veraltete Download-/Offline-Anzeigen** und falsche Offline-Daten (Finamp) (2.5).
9. **Wiedergabe-Aussetzer oder Stopp nach Netzverlust/OS-Suspend** (Substreamer, Finamp, Navidrome-Issue) (2.2, 2.5).
10. **"Alle zufällig" spielt nur einen Ausschnitt** (Arpeggi) (2.6).
11. **Überladene Oberfläche mit steiler Lernkurve** (Symfonium) (2.1) – Tiefe gehört in Einstellungen, nicht in die Hauptansichten.
12. **Kernfunktionen hinter Paywall/Werbung** (Substreamer Chromecast/Werbung) (2.2).
13. **Cache lädt ungefragt alles** und belegt Speicher (Amperfy, Symfonium-Bildcache) (2.6, [Symfonium Docs](https://docs.symfonium.app/wiki/other/offline-media-cache-downloads-rolling-cache-playback-cache-automatic-cache-and-more/)) → Cache-Größe sichtbar und begrenzbar im Profil → Downloads verwalten.
14. **"Zuletzt gesucht" spielt sofort ab statt zu öffnen** (Spotify) (1.3).
