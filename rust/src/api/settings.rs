//! App-Einstellungen: Typen + Laden/Speichern.
//!
//! WARUM in Rust und nicht mit `shared_preferences` in Dart?
//! Mehrere Teile brauchen dieselben Einstellungen: die UI (Theme, Gesten),
//! der Rust-Kern (Qualität beim Bauen der Stream-URL, Download-Regeln) und
//! später Hintergrund-Jobs ohne laufende UI (Downloads, Sync). Liegt alles an
//! einer Stelle im Kern, kann nichts auseinanderlaufen.
//!
//! Gespeichert wird als JSON im App-Datenverzeichnis. `#[serde(default)]`
//! sorgt dafür, dass neue Felder in späteren Versionen alte Dateien nicht
//! kaputt machen – fehlende Werte bekommen einfach ihren Standard.

use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::core::storage;

// ---------------------------------------------------------------------------
// Darstellung
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    Dark,
    /// Reines Schwarz für OLED – spart dort echten Strom, weil schwarze Pixel aus sind.
    Black,
    Light,
    /// Folgt dem System; im Dunkel-Fall gilt `system_dark_variant`.
    System,
}

/// Welches Dunkel bei `ThemeChoice::System` verwendet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DarkVariant {
    Dark,
    Black,
}

// ---------------------------------------------------------------------------
// Klangqualität
// ---------------------------------------------------------------------------

/// Zielformat. Die Umwandlung macht der SERVER (z. B. Navidrome mit ffmpeg),
/// die App fordert nur das Format an. Grund: Umwandeln auf dem Handy würde
/// Akku kosten und das Datenvolumen gar nicht sparen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioFormat {
    /// Datei unverändert (z. B. FLAC).
    Original,
    /// Beste Qualität pro MB – Standard für mobil und Downloads.
    Opus,
    Aac,
    Mp3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamQuality {
    pub format: AudioFormat,
    /// Wird bei `Original` ignoriert.
    pub bitrate_kbps: u32,
}

// ---------------------------------------------------------------------------
// Wischgesten
// ---------------------------------------------------------------------------

/// Was eine Wischgeste auf einer Zeile auslöst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SwipeAction {
    None,
    AddToQueue,
    ToggleFavorite,
    Download,
    AddToPlaylist,
    GoToAlbum,
    GoToArtist,
    /// Nur in der Warteschlange sinnvoll.
    RemoveFromQueue,
}

/// Belegung einer Zeilenart: kurz/lang je Richtung = bis zu 4 Aktionen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwipeSet {
    pub right_short: SwipeAction,
    pub right_long: SwipeAction,
    pub left_short: SwipeAction,
    pub left_long: SwipeAction,
}

/// Gesten auf Mini-Player und Cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerSwipeAction {
    None,
    NextTrack,
    PreviousTrack,
    OpenPlayer,
    TogglePlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerSwipes {
    pub left: PlayerSwipeAction,
    pub right: PlayerSwipeAction,
    pub up: PlayerSwipeAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwipeConfig {
    /// Titel-Zeilen in Album, Playlist, Suche, Favoriten, Downloads.
    pub track_rows: SwipeSet,
    /// Album- und Playlist-Zeilen.
    pub collection_rows: SwipeSet,
    /// Zeilen in der Warteschlange.
    pub queue_rows: SwipeSet,
    pub mini_player: PlayerSwipes,
}

// ---------------------------------------------------------------------------
// Gesamte Einstellungen
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: ThemeChoice,
    pub system_dark_variant: DarkVariant,
    /// Akzentfarbe der Oberfläche als 0xAARRGGBB. Der Nutzer wählt sie in
    /// den Einstellungen aus einer Palette.
    pub accent_color: u32,

    pub quality_wifi: StreamQuality,
    pub quality_mobile: StreamQuality,
    pub quality_download: StreamQuality,

    /// In mobilen Daten nur heruntergeladene Inhalte zeigen (abschaltbar).
    pub offline_on_mobile: bool,
    /// Downloads nur im WLAN.
    pub download_wifi_only: bool,
    /// Ohne Netz automatisch nur heruntergeladene Inhalte zeigen.
    pub auto_offline: bool,
    /// Obergrenze für Downloads in MB. 0 = keine Grenze.
    pub download_limit_mb: u32,
    /// Größe des Streaming-Caches in MB (zuletzt gestreamte Titel bleiben
    /// gespeichert und kosten beim erneuten Hören kein Datenvolumen). 0 = aus.
    pub stream_cache_mb: u32,
    /// Downloads auf der SD-Karte statt im internen Speicher ablegen.
    pub downloads_on_sd_card: bool,
    /// Neue Favoriten automatisch herunterladen.
    pub auto_sync_favorites: bool,
    /// Vorgemerkte Server-Playlists automatisch aktuell halten.
    pub auto_sync_playlists: bool,
    /// Smart-Playlists mit "automatisch herunterladen" aktuell halten.
    pub auto_sync_smart: bool,

    pub swipes: SwipeConfig,
}

impl Default for AppSettings {
    fn default() -> Self {
        use SwipeAction as S;
        Self {
            theme: ThemeChoice::System,
            system_dark_variant: DarkVariant::Dark,
            // Violett: dieselbe Farbe wie die Balken im App-Icon.
            accent_color: 0xFF8B7CFF,
            // Standardwerte so gewählt, dass im WLAN volle Qualität läuft und
            // unterwegs Datenvolumen gespart wird. Opus 128 klingt für die
            // meisten Ohren transparent.
            quality_wifi: StreamQuality { format: AudioFormat::Original, bitrate_kbps: 0 },
            quality_mobile: StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 128 },
            quality_download: StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 160 },
            offline_on_mobile: false,
            download_wifi_only: true,
            auto_offline: true,
            download_limit_mb: 0,
            stream_cache_mb: 512,
            downloads_on_sd_card: false,
            // Auto-Sync ist anfangs aus: Die App lädt nichts herunter, was
            // der Nutzer nicht ausdrücklich wollte.
            auto_sync_favorites: false,
            auto_sync_playlists: false,
            auto_sync_smart: false,
            swipes: SwipeConfig {
                // Wie bei Spotify: nach rechts = zur Warteschlange.
                track_rows: SwipeSet {
                    right_short: S::AddToQueue,
                    right_long: S::AddToPlaylist,
                    left_short: S::ToggleFavorite,
                    left_long: S::Download,
                },
                collection_rows: SwipeSet {
                    right_short: S::AddToQueue,
                    right_long: S::None,
                    left_short: S::Download,
                    left_long: S::None,
                },
                queue_rows: SwipeSet {
                    right_short: S::ToggleFavorite,
                    right_long: S::None,
                    left_short: S::RemoveFromQueue,
                    left_long: S::None,
                },
                mini_player: PlayerSwipes {
                    left: PlayerSwipeAction::NextTrack,
                    right: PlayerSwipeAction::PreviousTrack,
                    up: PlayerSwipeAction::OpenPlayer,
                },
            },
        }
    }
}

/// Zwischenspeicher im RAM, damit nicht bei jeder Stream-URL die Datei
/// gelesen werden muss.
static SETTINGS: LazyLock<RwLock<AppSettings>> =
    LazyLock::new(|| RwLock::new(AppSettings::default()));

fn settings_path() -> Result<PathBuf> {
    Ok(storage::data_dir()?.join("settings.json"))
}

/// Lädt die Einstellungen von der Platte (wird von `init_app` aufgerufen).
pub(crate) fn load_from_disk() -> Result<()> {
    let s: AppSettings = storage::read_json(&settings_path()?)?.unwrap_or_default();
    *SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = s;
    Ok(())
}

/// Aktuelle Einstellungen. `sync`, weil es nur eine Kopie aus dem RAM ist –
/// so kann die UI sie beim Start ohne `await` lesen.
#[flutter_rust_bridge::frb(sync)]
pub fn get_settings() -> AppSettings {
    SETTINGS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Speichert neue Einstellungen (RAM + Datei).
pub fn save_settings(settings: AppSettings) -> Result<()> {
    storage::write_json(&settings_path()?, &settings)?;
    *SETTINGS.write().unwrap_or_else(|e| e.into_inner()) = settings;
    Ok(())
}

/// Für interne Nutzung im Kern (z. B. Stream-Qualität).
pub(crate) fn current() -> AppSettings {
    SETTINGS.read().unwrap_or_else(|e| e.into_inner()).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_file_with_missing_fields_still_loads() {
        // Simuliert eine Einstellungsdatei aus einer älteren App-Version.
        let s: AppSettings = serde_json::from_str(r#"{"theme":"Black"}"#).unwrap();
        assert_eq!(s.theme, ThemeChoice::Black);
        assert_eq!(s.quality_mobile, AppSettings::default().quality_mobile);
    }
}
