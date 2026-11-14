//! Hörverlauf, Statistik und Scrobbling.
//!
//! Ablauf, wenn ein Titel gehört wurde (`report_play`):
//!  1. In den lokalen Verlauf schreiben (Grundlage für Statistik und später
//!     Smart-Playlists). Das klappt immer, auch offline.
//!  2. In die Scrobble-Warteschlange legen.
//!  3. Versuchen, die Warteschlange an den Server zu schicken. Klappt das
//!     nicht (kein Netz), bleibt der Eintrag liegen und wird beim nächsten
//!     Mal nachgereicht – mit dem ursprünglichen Zeitpunkt.

use anyhow::Result;

use crate::api::library;
use crate::api::models::Track;
use crate::core::db::{self, TopBy};
use crate::core::runtime;

/// Meldet dem Server "läuft gerade". Rein informativ, Fehler sind egal.
pub fn report_now_playing(track_id: String) {
    if let Ok(c) = library::client() {
        let _ = runtime::block_on_net(c.scrobble(&track_id, None, false));
    }
}

/// Ein Titel wurde gehört. `listened_secs` = tatsächlich gehörte Zeit.
///
/// Ob ein Titel als "gehört" zählt, entscheidet die Dart-Seite nach der
/// üblichen Scrobble-Regel (mindestens die Hälfte oder 4 Minuten).
pub fn report_play(track: Track, listened_secs: u32) -> Result<()> {
    let now = db::now_secs();
    db::record_play(&track, listened_secs, now)?;
    db::add_pending_scrobble(&track.id, now * 1000)?;
    flush_scrobbles();
    Ok(())
}

/// Schickt wartende Scrobbles an den Server, älteste zuerst.
///
/// Beim ersten Fehler wird abgebrochen: Ist der Server nicht erreichbar,
/// scheitern die restlichen auch, und jeder Versuch kostet Akku und Zeit.
pub fn flush_scrobbles() {
    // Nur EIN Nachreichen zur selben Zeit. Sonst könnten zwei gleichzeitige
    // Aufrufe (Titelende + Netzwechsel) dieselben wartenden Einträge lesen
    // und doppelt an den Server schicken. `try_lock`: Läuft schon eines,
    // ist nichts zu tun – es nimmt die neuen Einträge gleich mit.
    static FLUSHING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Ok(_guard) = FLUSHING.try_lock() else { return };
    let Ok(c) = library::client() else { return };
    let Ok(pending) = db::pending_scrobbles(50) else { return };
    for (row_id, track_id, time_ms) in pending {
        if runtime::block_on_net(c.scrobble(&track_id, Some(time_ms), true)).is_err() {
            break;
        }
        let _ = db::delete_pending_scrobble(row_id);
    }
}

/// Eine Zeile einer Bestenliste (Titel, Künstler oder Album).
#[derive(Debug, Clone)]
pub struct StatEntry {
    pub id: Option<String>,
    pub name: String,
    pub subtitle: Option<String>,
    pub cover_art_id: Option<String>,
    pub plays: u32,
    pub listened_secs: u32,
}

#[derive(Debug, Clone)]
pub struct DayCount {
    /// Kalendertag in Ortszeit, "JJJJ-MM-TT".
    pub day: String,
    pub plays: u32,
}

#[derive(Debug, Clone)]
pub struct ListeningStats {
    pub total_plays: u32,
    pub total_secs: u64,
    pub top_tracks: Vec<StatEntry>,
    pub top_artists: Vec<StatEntry>,
    pub top_albums: Vec<StatEntry>,
    /// Wiedergaben pro Tag (nur Tage mit mindestens einer Wiedergabe).
    pub days: Vec<DayCount>,
    /// Wiedergaben je Tagesstunde, immer 24 Einträge.
    pub hours: Vec<u32>,
}

fn entries(by: TopBy, since: i64) -> Result<Vec<StatEntry>> {
    Ok(db::top(by, since, 10)?
        .into_iter()
        .map(|r| StatEntry {
            id: r.id,
            name: r.name,
            subtitle: r.subtitle,
            cover_art_id: r.cover_art_id,
            plays: r.plays,
            listened_secs: r.listened_secs,
        })
        .collect())
}

/// Statistik über die letzten `days` Tage. `None` = gesamter Verlauf.
pub fn listening_stats(days: Option<u32>) -> Result<ListeningStats> {
    let since = days.map(|d| db::now_secs() - d as i64 * 86_400).unwrap_or(0);
    let (total_plays, total_secs) = db::totals(since)?;
    Ok(ListeningStats {
        total_plays,
        total_secs,
        top_tracks: entries(TopBy::Track, since)?,
        top_artists: entries(TopBy::Artist, since)?,
        top_albums: entries(TopBy::Album, since)?,
        days: db::plays_per_day(since)?
            .into_iter()
            .map(|(day, plays)| DayCount { day, plays })
            .collect(),
        hours: db::plays_per_hour(since)?,
    })
}

/// Ein Eintrag im Verlauf.
#[derive(Debug, Clone)]
pub struct PlayRecord {
    pub track: Track,
    pub played_at_ms: i64,
}

pub fn recent_plays(limit: u32) -> Result<Vec<PlayRecord>> {
    Ok(db::recent_plays(limit)?
        .into_iter()
        .map(|(track, at)| PlayRecord { track, played_at_ms: at * 1000 })
        .collect())
}
