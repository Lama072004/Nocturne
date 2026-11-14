//! Smart-Playlists: Playlists, die sich aus Regeln selbst zusammenstellen.
//!
//! Beispiel: (Genre ist "Rock" ODER Genre ist "Metal") UND Jahr > 1999
//!           UND seit mehr als 30 Tagen nicht gehört.
//!
//! # Woher kommen die Daten?
//! Die Regeln werden LOKAL ausgewertet, nicht am Server. Dafür hält die App
//! eine Kopie aller Titel-Metadaten in der Datenbank (`sync_library`).
//! Vorteile:
//!  - funktioniert offline,
//!  - funktioniert mit jedem Server gleich (Subsonic kennt keine Regeln),
//!  - kann den EIGENEN Hörverlauf der App einbeziehen ("zuletzt gehört").
//!
//! # Aufbau der Regeln
//! Eine [`RuleSet`] hat eigene Regeln und Untergruppen. Jede Ebene wählt
//! "alle müssen zutreffen" (UND) oder "eine muss zutreffen" (ODER). Eine
//! Ebene Untergruppen reicht für alle üblichen Fälle und hält den Editor
//! auf dem Handy bedienbar.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::api::library;
use crate::api::models::Track;
use crate::core::{db, runtime, smart};

/// Worauf sich eine Regel bezieht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleField {
    Title,
    Artist,
    Album,
    Genre,
    Year,
    /// Länge in Sekunden.
    Duration,
    /// Favorit (ja/nein).
    Starred,
    /// Bewertung 1–5 (0 = unbewertet).
    Rating,
    /// Wiedergaben: das Höhere aus Server-Zähler und eigenem Verlauf.
    PlayCount,
    /// Tage seit der letzten Wiedergabe (nie gehört = unendlich lange her).
    DaysSincePlayed,
    /// Tage seit dem Hinzufügen zur Bibliothek.
    DaysSinceAdded,
    /// Ist heruntergeladen (ja/nein).
    Downloaded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleOp {
    Is,
    IsNot,
    Contains,
    NotContains,
    Greater,
    Less,
    /// Für Ja/Nein-Felder.
    IsTrue,
    IsFalse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub field: RuleField,
    pub op: RuleOp,
    /// Vergleichswert als Text; Zahlen werden beim Auswerten umgewandelt.
    /// Bei Ja/Nein-Feldern leer.
    pub value: String,
}

/// Untergruppe: mehrere Regeln, verknüpft mit UND oder ODER.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RuleGroup {
    /// `true` = alle Regeln müssen zutreffen (UND), `false` = eine reicht (ODER).
    pub match_all: bool,
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuleSet {
    pub match_all: bool,
    pub rules: Vec<Rule>,
    pub groups: Vec<RuleGroup>,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self { match_all: true, rules: Vec::new(), groups: Vec::new() }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SmartSort {
    Random,
    Title,
    Artist,
    Album,
    YearNewest,
    AddedNewest,
    MostPlayed,
    LeastPlayed,
    /// Am längsten nicht gehört zuerst.
    PlayedLongestAgo,
    PlayedRecently,
    BestRated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartPlaylist {
    /// Leer bei einer neuen Playlist; wird beim Speichern vergeben.
    pub id: String,
    pub name: String,
    pub rules: RuleSet,
    pub sort: SmartSort,
    /// Höchstens so viele Titel.
    pub limit_count: Option<u32>,
    /// Höchstens so viele Minuten Gesamtlänge.
    pub limit_minutes: Option<u32>,
    /// Ergebnis zusätzlich als normale Playlist auf den Server schreiben,
    /// damit andere Geräte und Apps sie sehen.
    pub mirror_to_server: bool,
    /// ID der gespiegelten Playlist am Server (wird von der App gepflegt).
    pub server_playlist_id: Option<String>,
    /// Inhalt automatisch herunterladen.
    pub auto_download: bool,
}

impl Default for SmartPlaylist {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            rules: RuleSet::default(),
            sort: SmartSort::Random,
            limit_count: Some(50),
            limit_minutes: None,
            mirror_to_server: false,
            server_playlist_id: None,
            auto_download: false,
        }
    }
}

/// Stand der lokalen Titel-Kopie.
#[derive(Debug, Clone)]
pub struct LibrarySyncInfo {
    pub track_count: u32,
    /// Zeitpunkt des letzten erfolgreichen Abgleichs; `None` = noch nie.
    pub last_sync_ms: Option<i64>,
}

// ---------------------------------------------------------------------------
// Bibliothek abgleichen
// ---------------------------------------------------------------------------

/// Lädt die Metadaten ALLER Titel vom Server in die lokale Datenbank.
///
/// Es werden nur Texte übertragen (Titel, Künstler, Jahr …), keine Musik.
/// Selbst bei 50.000 Titeln sind das wenige MB. Trotzdem passiert es nicht
/// von selbst im Hintergrund, sondern nur, wenn die Oberfläche es anstößt.
///
/// Die alte Kopie wird erst ersetzt, wenn ALLE Seiten erfolgreich geladen
/// sind. Bricht das Netz mittendrin ab, bleibt der bisherige Stand erhalten.
pub fn sync_library() -> Result<u32> {
    const PAGE: u32 = 500;
    let client = library::client()?;
    let mut all: Vec<Track> = Vec::new();
    loop {
        let page = runtime::block_on_net(client.songs_page(all.len() as u32, PAGE))?;
        let n = page.len() as u32;
        all.extend(page);
        // Eine nicht volle Seite bedeutet: Ende erreicht.
        if n < PAGE {
            break;
        }
    }
    db::replace_library(&all)?;
    // Die Bibliotheks-Ansichten lesen ab jetzt den neuen Stand.
    crate::api::local::invalidate();
    db::set_meta("library_synced_at", &db::now_secs().to_string())?;
    Ok(all.len() as u32)
}

pub fn library_sync_info() -> Result<LibrarySyncInfo> {
    Ok(LibrarySyncInfo {
        track_count: db::library_count()?,
        last_sync_ms: db::get_meta("library_synced_at")?
            .and_then(|v| v.parse::<i64>().ok())
            .map(|s| s * 1000),
    })
}

// ---------------------------------------------------------------------------
// Smart-Playlists verwalten
// ---------------------------------------------------------------------------

pub fn list_smart_playlists() -> Result<Vec<SmartPlaylist>> {
    db::smart_playlists()
}

/// Speichert eine Smart-Playlist. Neue bekommen dabei eine ID.
pub fn save_smart_playlist(playlist: SmartPlaylist) -> Result<SmartPlaylist> {
    let mut p = playlist;
    if p.id.is_empty() {
        // Zeit + Zufall: eindeutig genug und ohne zusätzliches UUID-Crate.
        p.id = format!("sp-{}-{:04x}", db::now_secs(), rand::random::<u16>());
    }
    db::save_smart_playlist(&p)?;
    Ok(p)
}

pub fn delete_smart_playlist(id: String) -> Result<()> {
    db::delete_smart_playlist(&id)
}

/// Vorschau im Editor: wertet die Regeln aus, ohne etwas zu speichern oder
/// an den Server zu schicken.
pub fn preview_smart_playlist(playlist: SmartPlaylist) -> Result<Vec<Track>> {
    smart::evaluate(&playlist)
}

/// Wertet eine gespeicherte Smart-Playlist aus und spiegelt das Ergebnis
/// auf Wunsch an den Server.
pub fn evaluate_smart_playlist(id: String) -> Result<Vec<Track>> {
    let mut playlist = db::smart_playlists()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| anyhow!("Smart-Playlist nicht gefunden"))?;
    let tracks = smart::evaluate(&playlist)?;

    if playlist.mirror_to_server {
        // Fehler beim Spiegeln (offline) sollen das Abspielen nicht
        // verhindern; beim nächsten Öffnen wird es erneut versucht.
        if let Ok(client) = library::client() {
            let ids: Vec<String> = tracks.iter().map(|t| t.id.clone()).collect();
            let result = runtime::block_on_net(client.write_playlist(
                playlist.server_playlist_id.as_deref(),
                &playlist.name,
                &ids,
            ));
            if let Ok(Some(server_id)) = result {
                if playlist.server_playlist_id.as_deref() != Some(&server_id) {
                    playlist.server_playlist_id = Some(server_id);
                    let _ = db::save_smart_playlist(&playlist);
                }
            }
        }
    }
    Ok(tracks)
}

// ---------------------------------------------------------------------------
// Server-Playlists, die offline aktuell gehalten werden sollen
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PinnedPlaylist {
    pub id: String,
    pub name: String,
}

/// Merkt eine Server-Playlist für "offline aktuell halten" vor (oder nimmt
/// sie wieder heraus). Das eigentliche Nachladen macht der Auto-Sync in Dart.
pub fn set_playlist_pinned(id: String, name: String, pinned: bool) -> Result<()> {
    db::set_playlist_pinned(&id, &name, pinned)
}

pub fn pinned_playlists() -> Result<Vec<PinnedPlaylist>> {
    Ok(db::pinned_playlists()?
        .into_iter()
        .map(|(id, name)| PinnedPlaylist { id, name })
        .collect())
}
