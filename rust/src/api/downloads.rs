//! Metadaten der heruntergeladenen Titel.
//!
//! Arbeitsteilung bei Downloads:
//!  - Android (Media3 `DownloadService`) lädt die AUDIODATEI im Hintergrund,
//!    auch wenn die App geschlossen ist, und setzt abgebrochene Downloads fort.
//!  - Rust merkt sich hier, WELCHE Titel heruntergeladen sind (Name, Album,
//!    Cover …), damit der Downloads-Tab und die Offline-Wiedergabe ohne
//!    Server funktionieren.
//!
//! Beides wird von Dart (`DownloadController`) zusammengehalten.

use anyhow::Result;

use crate::api::models::Track;
use crate::core::db;

/// Merkt einen Titel als heruntergeladen vor.
pub fn save_download(track: Track) -> Result<()> {
    db::save_download(&track)
}

pub fn remove_download(track_id: String) -> Result<()> {
    db::remove_download(&track_id)
}

/// Alle heruntergeladenen Titel, zuletzt hinzugefügte zuerst.
pub fn downloaded_tracks() -> Result<Vec<Track>> {
    db::downloads()
}
