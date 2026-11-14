//! Lokale SQLite-Datenbank.
//!
//! Hier liegt alles, was der Server nicht (oder nicht offline) liefern kann:
//!  - `plays`             : eigener Hörverlauf -> Statistik-Tab und später Smart-Playlists
//!  - `pending_scrobbles` : Wiedergaben, die dem Server noch gemeldet werden müssen
//!                          (z. B. weil offline gehört wurde)
//!  - `downloads`         : Metadaten der heruntergeladenen Titel, damit der
//!                          Downloads-Tab ohne Netz funktioniert
//!
//! Die Audiodateien selbst liegen NICHT hier, sondern im Media3-Cache auf
//! der Android-Seite. Die Datenbank kennt nur, WAS heruntergeladen ist.
//!
//! WARUM SQLite und nicht JSON-Dateien wie bei den Einstellungen?
//! Der Verlauf wächst unbegrenzt, und die Statistik braucht Auswertungen
//! ("Top-Künstler der letzten 30 Tage"). Genau dafür ist SQL gemacht; eine
//! JSON-Datei müsste jedes Mal komplett gelesen und geschrieben werden.

use std::path::Path;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection};

use crate::api::models::Track;

/// Eine einzige Verbindung für die ganze App, hinter einem Mutex.
/// Die App schreibt selten (ein Eintrag pro gehörtem Titel); ein
/// Verbindungs-Pool wäre hier nur unnötiger Aufwand.
static DB: Mutex<Option<Connection>> = Mutex::new(None);

/// Aktuelle Schema-Version. Bei Änderungen am Schema hochzählen und in
/// `migrate` einen neuen Schritt ergänzen. Bestehende Daten bleiben so erhalten.
const SCHEMA_VERSION: i32 = 2;

pub fn open(dir: &Path) -> Result<()> {
    let conn = Connection::open(dir.join("library.db"))?;
    // WAL: Schreiben blockiert Lesen nicht, und ein Absturz mitten im
    // Schreiben beschädigt die Datenbank nicht.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    // NORMAL statt FULL: deutlich weniger Schreibzugriffe auf den Flash-
    // Speicher (Akku, Lebensdauer). Im schlimmsten Fall geht bei einem
    // Stromausfall der allerletzte Eintrag verloren, die DB bleibt heil.
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    *DB.lock().unwrap_or_else(|e| e.into_inner()) = Some(conn);
    Ok(())
}

/// Nur für Tests: Datenbank im Arbeitsspeicher.
#[cfg(test)]
pub fn open_in_memory() -> Result<()> {
    let conn = Connection::open_in_memory()?;
    migrate(&conn)?;
    *DB.lock().unwrap_or_else(|e| e.into_inner()) = Some(conn);
    Ok(())
}

/// Führt EINEN Migrationsschritt aus und merkt sich danach seine Nummer –
/// beides zusammen in einer Transaktion.
///
/// WARUM? Stürzt die App mitten in einer Migration ab, darf beim nächsten
/// Start nicht derselbe Schritt noch einmal laufen ("Tabelle existiert
/// schon" -> die App würde nie wieder starten). In der Transaktion gilt:
/// entweder ist der ganze Schritt samt neuer Nummer gespeichert oder gar
/// nichts davon.
fn step(conn: &Connection, version: i32, sql: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch(sql)?;
    tx.pragma_update(None, "user_version", version)?;
    tx.commit()?;
    Ok(())
}

fn migrate(conn: &Connection) -> Result<()> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version < 1 {
        step(conn, 1,
            "CREATE TABLE plays (
                id            INTEGER PRIMARY KEY,
                track_id      TEXT NOT NULL,
                -- Namen zusätzlich als Spalten, damit die Statistik gruppieren
                -- kann, ohne das JSON jedes Eintrags zu lesen.
                title         TEXT NOT NULL,
                artist        TEXT,
                artist_id     TEXT,
                album         TEXT,
                album_id      TEXT,
                cover_art_id  TEXT,
                -- Unix-Zeit in Sekunden
                played_at     INTEGER NOT NULL,
                listened_secs INTEGER NOT NULL,
                -- Kompletter Titel, damit man ihn aus dem Verlauf abspielen kann.
                track_json    TEXT NOT NULL
            );
            CREATE INDEX plays_played_at ON plays(played_at);

            CREATE TABLE pending_scrobbles (
                id       INTEGER PRIMARY KEY,
                track_id TEXT NOT NULL,
                time_ms  INTEGER NOT NULL
            );

            CREATE TABLE downloads (
                track_id   TEXT PRIMARY KEY,
                track_json TEXT NOT NULL,
                added_at   INTEGER NOT NULL
            );",
        )?;
    }
    if version < 2 {
        step(conn, 2,
            "-- Lokale Kopie aller Titel-Metadaten des Servers. Darauf rechnen
            -- die Smart-Playlists (auch offline).
            CREATE TABLE library (
                track_id   TEXT PRIMARY KEY,
                track_json TEXT NOT NULL
            );
            CREATE TABLE smart_playlists (
                id         TEXT PRIMARY KEY,
                json       TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            -- Kleine Schlüssel/Wert-Ablage, z. B. Zeitpunkt des letzten Abgleichs.
            CREATE TABLE meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            -- Server-Playlists, die offline aktuell gehalten werden sollen.
            CREATE TABLE pinned_playlists (
                id   TEXT PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE INDEX plays_track ON plays(track_id);",
        )?;
    }
    debug_assert!(SCHEMA_VERSION == 2, "neuer Schritt in migrate() vergessen?");
    Ok(())
}

/// Führt `f` mit der Datenbankverbindung aus.
pub fn with<T>(f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let guard = DB.lock().unwrap_or_else(|e| e.into_inner());
    let conn = guard.as_ref().ok_or_else(|| anyhow!("Datenbank ist nicht geöffnet"))?;
    f(conn)
}

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Hörverlauf
// ---------------------------------------------------------------------------

pub fn record_play(track: &Track, listened_secs: u32, played_at: i64) -> Result<()> {
    with(|c| {
        c.execute(
            "INSERT INTO plays (track_id, title, artist, artist_id, album, album_id,
                                cover_art_id, played_at, listened_secs, track_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                track.id,
                track.title,
                track.artist,
                track.artist_id,
                track.album,
                track.album_id,
                track.cover_art_id,
                played_at,
                listened_secs,
                serde_json::to_string(track)?,
            ],
        )?;
        Ok(())
    })
}

/// Zuletzt gehörte Titel, neueste zuerst: (Titel, Zeitpunkt in Sekunden).
pub fn recent_plays(limit: u32) -> Result<Vec<(Track, i64)>> {
    with(|c| {
        let mut stmt =
            c.prepare("SELECT track_json, played_at FROM plays ORDER BY played_at DESC LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (json, at) = row?;
            // Ein kaputter Eintrag soll nicht den ganzen Verlauf unlesbar machen.
            if let Ok(t) = serde_json::from_str::<Track>(&json) {
                out.push((t, at));
            }
        }
        Ok(out)
    })
}

/// Eine Zeile einer Bestenliste.
pub struct TopRow {
    pub id: Option<String>,
    pub name: String,
    pub subtitle: Option<String>,
    pub cover_art_id: Option<String>,
    pub plays: u32,
    pub listened_secs: u32,
}

/// Wonach die Bestenliste gruppiert wird.
#[derive(Clone, Copy)]
pub enum TopBy {
    Track,
    Artist,
    Album,
}

pub fn top(by: TopBy, since: i64, limit: u32) -> Result<Vec<TopRow>> {
    // Die Spaltennamen kommen aus diesem festen `match`, nie aus
    // Nutzereingaben. Deshalb ist das Zusammensetzen des SQL hier sicher.
    let (id, name, subtitle, group) = match by {
        TopBy::Track => ("track_id", "title", "artist", "track_id"),
        TopBy::Artist => ("artist_id", "artist", "NULL", "artist"),
        TopBy::Album => ("album_id", "album", "artist", "COALESCE(album_id, album)"),
    };
    let sql = format!(
        "SELECT {id}, {name}, {subtitle}, MAX(cover_art_id), COUNT(*), SUM(listened_secs)
         FROM plays
         WHERE played_at >= ?1 AND {name} IS NOT NULL
         GROUP BY {group}
         ORDER BY COUNT(*) DESC, SUM(listened_secs) DESC
         LIMIT ?2"
    );
    with(|c| {
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(params![since, limit], |r| {
            Ok(TopRow {
                id: r.get(0)?,
                name: r.get(1)?,
                subtitle: r.get(2)?,
                cover_art_id: r.get(3)?,
                plays: r.get(4)?,
                listened_secs: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// (Anzahl Wiedergaben, gehörte Sekunden) seit `since`.
pub fn totals(since: i64) -> Result<(u32, u64)> {
    with(|c| {
        Ok(c.query_row(
            "SELECT COUNT(*), COALESCE(SUM(listened_secs), 0) FROM plays WHERE played_at >= ?1",
            [since],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    })
}

/// Wiedergaben pro Kalendertag ("2026-11-13", Anzahl) in Ortszeit, für die Heatmap.
pub fn plays_per_day(since: i64) -> Result<Vec<(String, u32)>> {
    with(|c| {
        let mut stmt = c.prepare(
            "SELECT strftime('%Y-%m-%d', played_at, 'unixepoch', 'localtime') AS day, COUNT(*)
             FROM plays WHERE played_at >= ?1 GROUP BY day ORDER BY day",
        )?;
        let rows = stmt.query_map([since], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

/// Wiedergaben je Stunde des Tages (Index 0..=23) in Ortszeit.
pub fn plays_per_hour(since: i64) -> Result<Vec<u32>> {
    with(|c| {
        let mut stmt = c.prepare(
            "SELECT CAST(strftime('%H', played_at, 'unixepoch', 'localtime') AS INTEGER), COUNT(*)
             FROM plays WHERE played_at >= ?1 GROUP BY 1",
        )?;
        let mut hours = vec![0u32; 24];
        let rows = stmt.query_map([since], |r| Ok((r.get::<_, usize>(0)?, r.get::<_, u32>(1)?)))?;
        for row in rows {
            let (h, n) = row?;
            if h < 24 {
                hours[h] = n;
            }
        }
        Ok(hours)
    })
}

// ---------------------------------------------------------------------------
// Scrobble-Warteschlange
// ---------------------------------------------------------------------------

pub fn add_pending_scrobble(track_id: &str, time_ms: i64) -> Result<()> {
    with(|c| {
        c.execute(
            "INSERT INTO pending_scrobbles (track_id, time_ms) VALUES (?1, ?2)",
            params![track_id, time_ms],
        )?;
        Ok(())
    })
}

/// Älteste zuerst, damit der Server die Wiedergaben in der richtigen
/// Reihenfolge bekommt: (Zeilen-ID, Titel-ID, Zeit in ms).
pub fn pending_scrobbles(limit: u32) -> Result<Vec<(i64, String, i64)>> {
    with(|c| {
        let mut stmt =
            c.prepare("SELECT id, track_id, time_ms FROM pending_scrobbles ORDER BY id LIMIT ?1")?;
        let rows = stmt.query_map([limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

pub fn delete_pending_scrobble(id: i64) -> Result<()> {
    with(|c| {
        c.execute("DELETE FROM pending_scrobbles WHERE id = ?1", [id])?;
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Downloads (nur Metadaten)
// ---------------------------------------------------------------------------

pub fn save_download(track: &Track) -> Result<()> {
    with(|c| {
        c.execute(
            "INSERT INTO downloads (track_id, track_json, added_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(track_id) DO UPDATE SET track_json = excluded.track_json",
            params![track.id, serde_json::to_string(track)?, now_secs()],
        )?;
        Ok(())
    })
}

pub fn remove_download(track_id: &str) -> Result<()> {
    with(|c| {
        c.execute("DELETE FROM downloads WHERE track_id = ?1", [track_id])?;
        Ok(())
    })
}

/// Alle heruntergeladenen Titel, zuletzt hinzugefügte zuerst.
pub fn downloads() -> Result<Vec<Track>> {
    with(|c| {
        let mut stmt = c.prepare("SELECT track_json FROM downloads ORDER BY added_at DESC, rowid DESC")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(t) = serde_json::from_str::<Track>(&row?) {
                out.push(t);
            }
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// Lokale Titel-Kopie und Smart-Playlists
// ---------------------------------------------------------------------------

/// Ersetzt die lokale Titel-Kopie in EINER Transaktion: Entweder ist danach
/// der komplette neue Stand da oder weiterhin der alte, nie ein halber.
pub fn replace_library(tracks: &[Track]) -> Result<()> {
    with(|c| {
        let tx = c.unchecked_transaction()?;
        tx.execute("DELETE FROM library", [])?;
        {
            // Vorbereitete Anweisung: SQLite muss das SQL nur einmal
            // übersetzen statt für jeden der vielen tausend Titel neu.
            let mut stmt =
                tx.prepare("INSERT OR REPLACE INTO library (track_id, track_json) VALUES (?1, ?2)")?;
            for t in tracks {
                stmt.execute(params![t.id, serde_json::to_string(t)?])?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}

pub fn library_tracks() -> Result<Vec<Track>> {
    with(|c| {
        let mut stmt = c.prepare("SELECT track_json FROM library")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(t) = serde_json::from_str::<Track>(&row?) {
                out.push(t);
            }
        }
        Ok(out)
    })
}

pub fn library_count() -> Result<u32> {
    with(|c| Ok(c.query_row("SELECT COUNT(*) FROM library", [], |r| r.get(0))?))
}

/// Eigene Statistik je Titel: ID -> (Anzahl Wiedergaben, letzte Wiedergabe).
pub fn local_play_stats() -> Result<std::collections::HashMap<String, (u32, i64)>> {
    with(|c| {
        let mut stmt =
            c.prepare("SELECT track_id, COUNT(*), MAX(played_at) FROM plays GROUP BY track_id")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, (r.get::<_, u32>(1)?, r.get::<_, i64>(2)?)))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    })
}

pub fn get_meta(key: &str) -> Result<Option<String>> {
    with(|c| {
        let mut stmt = c.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query([key])?;
        Ok(match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        })
    })
}

pub fn delete_meta(key: &str) -> Result<()> {
    with(|c| {
        c.execute("DELETE FROM meta WHERE key = ?1", [key])?;
        Ok(())
    })
}

pub fn set_meta(key: &str, value: &str) -> Result<()> {
    with(|c| {
        c.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    })
}

pub fn smart_playlists() -> Result<Vec<crate::api::smart::SmartPlaylist>> {
    with(|c| {
        let mut stmt = c.prepare("SELECT json FROM smart_playlists ORDER BY created_at")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(p) = serde_json::from_str(&row?) {
                out.push(p);
            }
        }
        Ok(out)
    })
}

pub fn save_smart_playlist(p: &crate::api::smart::SmartPlaylist) -> Result<()> {
    with(|c| {
        // Bei einer bestehenden Playlist bleibt `created_at` unverändert,
        // damit sie ihren Platz in der Liste behält.
        c.execute(
            "INSERT INTO smart_playlists (id, json, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET json = excluded.json",
            params![p.id, serde_json::to_string(p)?, now_secs()],
        )?;
        Ok(())
    })
}

pub fn delete_smart_playlist(id: &str) -> Result<()> {
    with(|c| {
        c.execute("DELETE FROM smart_playlists WHERE id = ?1", [id])?;
        Ok(())
    })
}

pub fn set_playlist_pinned(id: &str, name: &str, pinned: bool) -> Result<()> {
    with(|c| {
        if pinned {
            c.execute(
                "INSERT INTO pinned_playlists (id, name) VALUES (?1, ?2)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name",
                params![id, name],
            )?;
        } else {
            c.execute("DELETE FROM pinned_playlists WHERE id = ?1", [id])?;
        }
        Ok(())
    })
}

pub fn pinned_playlists() -> Result<Vec<(String, String)>> {
    with(|c| {
        let mut stmt = c.prepare("SELECT id, name FROM pinned_playlists ORDER BY name")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, artist: &str) -> Track {
        Track {
            id: id.into(),
            title: format!("Titel {id}"),
            artist: Some(artist.into()),
            ..Default::default()
        }
    }

    /// Alle DB-Tests in EINER Funktion: Sie teilen sich die globale
    /// Verbindung und würden sich sonst gegenseitig stören, weil `cargo test`
    /// Tests parallel ausführt.
    #[test]
    fn history_stats_scrobbles_and_downloads() {
        open_in_memory().unwrap();

        record_play(&t("1", "A"), 200, 1_000).unwrap();
        record_play(&t("1", "A"), 100, 2_000).unwrap();
        record_play(&t("2", "B"), 50, 3_000).unwrap();

        // Neueste zuerst.
        let recent = recent_plays(10).unwrap();
        assert_eq!(recent[0].0.id, "2");

        // Titel 1 wurde zweimal gehört -> steht oben.
        let top_tracks = top(TopBy::Track, 0, 10).unwrap();
        assert_eq!(top_tracks[0].name, "Titel 1");
        assert_eq!(top_tracks[0].plays, 2);
        assert_eq!(top_tracks[0].listened_secs, 300);

        // Zeitfilter: ab 2500 zählt nur noch der letzte Eintrag.
        assert_eq!(totals(2_500).unwrap(), (1, 50));
        assert_eq!(top(TopBy::Artist, 0, 10).unwrap()[0].name, "A");

        add_pending_scrobble("1", 111).unwrap();
        add_pending_scrobble("2", 222).unwrap();
        let pending = pending_scrobbles(10).unwrap();
        assert_eq!(pending.len(), 2);
        delete_pending_scrobble(pending[0].0).unwrap();
        assert_eq!(pending_scrobbles(10).unwrap()[0].1, "2");

        save_download(&t("9", "Z")).unwrap();
        save_download(&t("9", "Z")).unwrap(); // doppelt speichern = kein Duplikat
        assert_eq!(downloads().unwrap().len(), 1);
        remove_download("9").unwrap();
        assert!(downloads().unwrap().is_empty());

        // Lokale Titel-Kopie: Ersetzen wirft den alten Stand komplett weg.
        replace_library(&[t("1", "A"), t("2", "B")]).unwrap();
        replace_library(&[t("3", "C")]).unwrap();
        assert_eq!(library_count().unwrap(), 1);
        assert_eq!(library_tracks().unwrap()[0].id, "3");

        // Eigene Statistik: Titel 1 wurde oben zweimal gehört, zuletzt bei 2000.
        assert_eq!(local_play_stats().unwrap()["1"], (2, 2_000));

        set_meta("k", "a").unwrap();
        set_meta("k", "b").unwrap();
        assert_eq!(get_meta("k").unwrap().as_deref(), Some("b"));
        assert_eq!(get_meta("fehlt").unwrap(), None);
        delete_meta("k").unwrap();
        assert_eq!(get_meta("k").unwrap(), None);

        let p = crate::api::smart::SmartPlaylist {
            id: "sp1".into(),
            name: "Test".into(),
            ..Default::default()
        };
        save_smart_playlist(&p).unwrap();
        save_smart_playlist(&p).unwrap(); // erneut speichern = kein Duplikat
        assert_eq!(smart_playlists().unwrap().len(), 1);
        delete_smart_playlist("sp1").unwrap();
        assert!(smart_playlists().unwrap().is_empty());

        set_playlist_pinned("pl1", "Mix", true).unwrap();
        assert_eq!(pinned_playlists().unwrap(), [("pl1".to_owned(), "Mix".to_owned())]);
        set_playlist_pinned("pl1", "Mix", false).unwrap();
        assert!(pinned_playlists().unwrap().is_empty());
    }
}
