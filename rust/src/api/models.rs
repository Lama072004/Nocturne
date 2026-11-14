//! Gemeinsame Datentypen, die zwischen Rust und Dart ausgetauscht werden.
//!
//! WARUM liegen die Typen im `api`-Modul? flutter_rust_bridge erzeugt Dart-
//! Klassen für alles, was es unter `crate::api` findet. Liegen sie hier, sind
//! sie automatisch auf beiden Seiten identisch – keine doppelte Pflege.
//!
//! Die Typen sind bewusst server-NEUTRAL: Egal ob Navidrome, Jellyfin oder
//! Plex – die UI sieht immer dieselben Strukturen. Jeder Server-Adapter
//! übersetzt seine eigene Antwort in diese Typen.
//!
//! `Serialize`/`Deserialize` brauchen wir, um Titel in der lokalen Datenbank
//! abzulegen (Downloads, Hörverlauf): So lassen sie sich auch offline anzeigen.

use serde::{Deserialize, Serialize};

/// Welche Art von Server angebunden ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerKind {
    /// Navidrome, Gonic, Airsonic, Ampache, Nextcloud Music … (Subsonic-API)
    Subsonic,
    Jellyfin,
    Emby,
    Plex,
}

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub kind: ServerKind,
    /// Z. B. "navidrome" – OpenSubsonic-Server melden ihren Namen.
    pub server_type: Option<String>,
    pub server_version: Option<String>,
    pub api_version: String,
    /// OpenSubsonic-Erweiterungen (synchrone Songtexte, ReplayGain-Felder …).
    pub open_subsonic: bool,
}

/// ReplayGain-Werte eines Titels (aus den Datei-Tags, vom Server geliefert).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReplayGainInfo {
    pub track_gain_db: Option<f32>,
    pub album_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_peak: Option<f32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub album: Option<String>,
    pub album_id: Option<String>,
    /// ID für das Cover. Nicht immer gleich der Album-ID, deshalb getrennt.
    pub cover_art_id: Option<String>,
    pub duration_secs: Option<u32>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub starred: bool,
    pub replay_gain: Option<ReplayGainInfo>,
    /// Bewertung 1–5 am Server (für Smart-Playlists).
    pub user_rating: Option<u32>,
    /// Wiedergaben laut Server (zählt auch andere Apps mit).
    pub play_count: Option<u32>,
    /// Wann der Titel zur Bibliothek kam (Unix-Sekunden).
    pub added_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: Option<String>,
    pub artist_id: Option<String>,
    pub cover_art_id: Option<String>,
    pub year: Option<u32>,
    pub song_count: Option<u32>,
    pub duration_secs: Option<u32>,
    pub starred: bool,
}

#[derive(Debug, Clone)]
pub struct AlbumDetail {
    pub album: Album,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub cover_art_id: Option<String>,
    pub album_count: Option<u32>,
    pub starred: bool,
}

#[derive(Debug, Clone)]
pub struct ArtistDetail {
    pub artist: Artist,
    pub albums: Vec<Album>,
}

#[derive(Debug, Clone, Default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub song_count: Option<u32>,
    pub duration_secs: Option<u32>,
    pub cover_art_id: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
}

/// Alle Favoriten des Nutzers.
#[derive(Debug, Clone, Default)]
pub struct Starred {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
}

/// Eine Zeile Songtext.
#[derive(Debug, Clone)]
pub struct LyricLine {
    /// Startzeit in ms. `None` bei nicht synchronisierten Texten.
    pub start_ms: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    /// `true` = Zeilen haben Zeitstempel und können mitlaufen.
    pub synced: bool,
    pub lines: Vec<LyricLine>,
}

/// Was favorisiert werden kann (Subsonic braucht je Art einen anderen Parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StarKind {
    Track,
    Album,
    Artist,
}

/// Wiederholen wie bei Spotify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    /// Album/Playlist beginnt nach dem letzten Titel von vorne.
    All,
    /// Der aktuelle Titel läuft in Dauerschleife.
    One,
}
