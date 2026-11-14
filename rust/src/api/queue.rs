//! Warteschlangen-Schnittstelle für Dart.
//!
//! Die eigentliche Logik steht in `core::queue`. Hier wird nur:
//!  1. die globale Warteschlange gesperrt,
//!  2. die Operation ausgeführt,
//!  3. ein [`QueueSnapshot`] für die UI zurückgegeben.
//!
//! Jede Änderung liefert den kompletten Schnappschuss zurück. Die UI zeigt
//! also immer genau den Zustand, den Rust hat, und muss nie selbst etwas
//! nachrechnen (eine Fehlerquelle weniger).

use std::sync::{LazyLock, Mutex};

use serde::Serialize;

use crate::api::models::{RepeatMode, Track};
use crate::core::queue::{PlayQueue, QueueEntry, SavedQueue};
use crate::core::{browse, db, storage};

static QUEUE: LazyLock<Mutex<PlayQueue>> = LazyLock::new(|| Mutex::new(PlayQueue::default()));

/// Sperrt die Warteschlange. Ein "vergiftetes" Schloss (ein anderer Thread
/// ist mit gehaltener Sperre abgestürzt) wird übernommen statt die App zu
/// beenden: Die Warteschlange ist dann schlimmstenfalls einen Schritt alt.
fn lock() -> std::sync::MutexGuard<'static, PlayQueue> {
    QUEUE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Ein Platz in der Warteschlange, wie ihn die UI anzeigt.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub entry_id: String,
    pub track: Track,
}

/// Was ExoPlayer für einen Eintrag braucht. Die URLs baut Rust, weil nur
/// der Kern die Anmeldedaten und die gewünschte Qualität kennt.
///
/// `Serialize` mit festen Feldnamen: Dieselbe Struktur geht als JSON direkt
/// an den Android-Dienst (über JNI), wenn die Wiedergabe ohne die
/// Flutter-Oberfläche gestartet wird (Android Auto, Widget, Neustart).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeItem {
    pub entry_id: String,
    /// Song-ID des Servers = Cache-Schlüssel auf der Android-Seite. Darüber
    /// findet der Player einen heruntergeladenen Titel wieder.
    pub song_id: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// ID des Covers. Der Android-Dienst macht daraus eine eigene
    /// `content://`-Adresse; die echte Server-Adresse (mit Anmeldedaten)
    /// verlässt die App so nie über die Mediensitzung.
    pub cover_id: Option<String>,
    /// ReplayGain-Werte des Titels. Sie reisen mit dem Eintrag zu ExoPlayer;
    /// beim Titelwechsel gibt der Android-Dienst sie an die Effektkette.
    #[serde(rename = "trackGain")]
    pub track_gain_db: Option<f32>,
    #[serde(rename = "albumGain")]
    pub album_gain_db: Option<f32>,
    pub track_peak: Option<f32>,
    pub album_peak: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct QueueSnapshot {
    pub current: Option<QueueItem>,
    /// "Als Nächstes in der Warteschlange"
    pub user_queue: Vec<QueueItem>,
    /// "Als Nächstes von: `context_name`"
    pub context: Vec<QueueItem>,
    pub context_name: Option<String>,
    /// Flache Liste für ExoPlayer: Verlauf + aktuell + Warteschlange + Kontext.
    pub native_items: Vec<NativeItem>,
    /// Index des aktuellen Titels in `native_items`.
    pub native_index: u32,
    /// `true` = ExoPlayer braucht eine neue Liste (siehe `sync_to_entry`).
    pub needs_native_update: bool,
    pub shuffle: bool,
    pub repeat: RepeatMode,
}

fn item(e: &QueueEntry) -> QueueItem {
    QueueItem { entry_id: e.entry_id.clone(), track: e.track.clone() }
}

fn native(e: &QueueEntry) -> NativeItem {
    let t = &e.track;
    NativeItem {
        entry_id: e.entry_id.clone(),
        song_id: t.id.clone(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album: t.album.clone(),
        cover_id: t.cover_art_id.clone(),
        track_gain_db: t.replay_gain.as_ref().and_then(|r| r.track_gain_db),
        album_gain_db: t.replay_gain.as_ref().and_then(|r| r.album_gain_db),
        track_peak: t.replay_gain.as_ref().and_then(|r| r.track_peak),
        album_peak: t.replay_gain.as_ref().and_then(|r| r.album_peak),
    }
}

fn snapshot(q: &PlayQueue, needs_native_update: bool) -> QueueSnapshot {
    let (flat, idx) = q.flatten();
    QueueSnapshot {
        current: q.current.as_ref().map(item),
        user_queue: q.user_queue.iter().map(item).collect(),
        context: q.context.iter().map(item).collect(),
        context_name: q.context_name.clone(),
        native_items: flat.into_iter().map(native).collect(),
        native_index: idx as u32,
        needs_native_update,
        shuffle: q.shuffle,
        repeat: q.repeat,
    }
}

/// Album/Playlist ab Titel `start` abspielen.
pub fn queue_play_context(tracks: Vec<Track>, start: u32, name: Option<String>) -> QueueSnapshot {
    let mut q = lock();
    q.play_context(tracks, start as usize, name);
    persist(&q);
    snapshot(&q, true)
}

/// "Zur Warteschlange".
pub fn queue_add(tracks: Vec<Track>) -> QueueSnapshot {
    let mut q = lock();
    q.add_to_queue(tracks);
    persist(&q);
    snapshot(&q, true)
}

/// Drag & Drop in der Warteschlange (Positionen in `user_queue ++ context`).
pub fn queue_move(from: u32, to: u32) -> QueueSnapshot {
    let mut q = lock();
    let changed = q.move_upcoming(from as usize, to as usize);
    persist(&q);
    snapshot(&q, changed)
}

pub fn queue_remove(index: u32) -> QueueSnapshot {
    let mut q = lock();
    let changed = q.remove_upcoming(index as usize);
    persist(&q);
    snapshot(&q, changed)
}

pub fn queue_clear_user() -> QueueSnapshot {
    let mut q = lock();
    q.clear_user_queue();
    persist(&q);
    snapshot(&q, true)
}

/// ExoPlayer hat zu Eintrag `entry_id` gewechselt (automatisch, per
/// Benachrichtigung, Kopfhörertaste …). Rust zieht nach.
///
/// `wrapped` = ExoPlayer ist am Ende der Liste von selbst wieder nach vorne
/// gesprungen ("Alle wiederholen"), siehe `PlayQueue::sync_to_entry`.
pub fn queue_on_native_transition(entry_id: String, wrapped: bool) -> QueueSnapshot {
    let mut q = lock();
    let needs = q.sync_to_entry(&entry_id, wrapped);
    persist(&q);
    // Neuer Titel -> die gemerkte Abspielposition gilt nicht mehr.
    let _ = db::set_meta(POSITION_KEY, "0");
    snapshot(&q, needs)
}

/// Fügt einen Titel wieder in die kommenden Titel ein ("Rückgängig" nach
/// dem Entfernen). `index` zählt in `user_queue ++ context`.
pub fn queue_insert(index: u32, track: Track, into_user: bool) -> QueueSnapshot {
    let mut q = lock();
    q.insert_upcoming(index as usize, track, into_user);
    persist(&q);
    snapshot(&q, true)
}

/// "Quelle beenden": verwirft den Rest des Albums/der Playlist.
pub fn queue_clear_context() -> QueueSnapshot {
    let mut q = lock();
    q.clear_context();
    persist(&q);
    snapshot(&q, true)
}

/// Shuffle ein/aus (mischt nur den Kontext, nicht die eigene Warteschlange).
pub fn queue_set_shuffle(on: bool) -> QueueSnapshot {
    let mut q = lock();
    q.set_shuffle(on);
    persist(&q);
    snapshot(&q, true)
}

/// Wiederholen umschalten. Die Liste selbst ändert sich nicht; ExoPlayer
/// bekommt den Modus getrennt über den Plattform-Kanal.
pub fn queue_set_repeat(mode: RepeatMode) -> QueueSnapshot {
    let mut q = lock();
    q.set_repeat(mode);
    persist(&q);
    snapshot(&q, false)
}

pub fn queue_snapshot() -> QueueSnapshot {
    snapshot(&lock(), false)
}

// ---------------------------------------------------------------------------
// Speichern und Wiederherstellen
// ---------------------------------------------------------------------------

const POSITION_KEY: &str = "queue_position_ms";

fn queue_path() -> Option<std::path::PathBuf> {
    storage::data_dir().ok().map(|d| d.join("queue.json"))
}

/// Schreibt die Warteschlange auf die Platte.
///
/// Passiert nach jeder Änderung und jedem Titelwechsel – also höchstens
/// alle paar Minuten bzw. bei einer Nutzer-Aktion, nie in einer Schleife.
/// Fehler werden ignoriert: Ein nicht gespeicherter Stand kostet nur das
/// "Weiterspielen" nach einem Neustart, nicht die laufende Wiedergabe.
fn persist(q: &PlayQueue) {
    if let Some(path) = queue_path() {
        let _ = storage::write_json(&path, &q.to_saved());
    }
}

/// Lädt die zuletzt gespeicherte Warteschlange (beim Start des Kerns).
pub(crate) fn restore_from_disk() {
    let Some(path) = queue_path() else { return };
    if let Ok(Some(saved)) = storage::read_json::<SavedQueue>(&path) {
        let mut q = lock();
        // Nur übernehmen, wenn noch nichts läuft (der Kern kann von Dart
        // und vom Dienst eingerichtet werden; nur der Erste lädt).
        if q.current.is_none() {
            *q = PlayQueue::from_saved(saved);
        }
    }
}

// ---------------------------------------------------------------------------
// Für den Android-Dienst (über JNI, ohne Flutter)
// ---------------------------------------------------------------------------

/// Was der Dienst zum Abspielen braucht: Liste, Startindex, Startposition.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NativePayload {
    index: usize,
    position_ms: i64,
    items: Vec<NativeItem>,
}

fn payload(q: &PlayQueue, position_ms: i64) -> Option<String> {
    let (flat, index) = q.flatten();
    if flat.is_empty() {
        return None;
    }
    serde_json::to_string(&NativePayload {
        index,
        position_ms,
        items: flat.into_iter().map(native).collect(),
    })
    .ok()
}

/// Spielt einen Eintrag aus dem Inhaltsbaum (Android Auto, Widget, Sprache):
/// setzt die Warteschlange und liefert sie als JSON für ExoPlayer.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn play_from_media_id(media_id: &str) -> Option<String> {
    let (tracks, index, name) = browse::resolve(media_id).ok()?;
    let mut q = lock();
    q.play_context(tracks, index, name);
    persist(&q);
    let _ = db::set_meta(POSITION_KEY, "0");
    payload(&q, 0)
}

/// Die gespeicherte Warteschlange samt Position, für "Weiterspielen".
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn resume_json() -> Option<String> {
    let position = db::get_meta(POSITION_KEY).ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(0);
    payload(&lock(), position)
}

/// Merkt sich die Abspielposition des laufenden Titels.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn save_position(position_ms: i64) {
    let _ = db::set_meta(POSITION_KEY, &position_ms.max(0).to_string());
}

/// Der Titel zu einem Warteschlangen-Eintrag (für Verlauf und Scrobbling
/// aus dem Dienst, der nur die Eintrags-ID kennt).
///
/// Erst in der Warteschlange suchen, dann im Kurzzeit-Merker [`PLAYED`]:
/// Startet der Nutzer mitten im Titel ein anderes Album, ist der alte
/// Eintrag schon aus der Warteschlange verschwunden, wenn der Dienst ihn
/// kurz danach als "gehört" meldet. Ohne den Merker fehlte dieser Titel im
/// Verlauf.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn track_for_entry(entry_id: &str) -> Option<Track> {
    if let Some(track) = lock().find_entry(entry_id).map(|e| e.track.clone()) {
        return Some(track);
    }
    PLAYED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(id, _)| id == entry_id)
        .map(|(_, t)| t.clone())
}

/// Die zuletzt angespielten Einträge (Eintrags-ID, Titel), neueste hinten.
/// Bewusst klein: Es geht nur um die Sekunden zwischen "neue Warteschlange"
/// und "alter Titel wird gemeldet".
static PLAYED: Mutex<Vec<(String, Track)>> = Mutex::new(Vec::new());
const PLAYED_MAX: usize = 16;

/// Der Dienst meldet: Dieser Eintrag ist jetzt zu hören. Liefert den Titel
/// (für "Läuft gerade" an den Server) und merkt ihn sich für die spätere
/// Meldung "gehört".
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn note_playing(entry_id: &str) -> Option<Track> {
    let track = lock().find_entry(entry_id).map(|e| e.track.clone())?;
    let mut played = PLAYED.lock().unwrap_or_else(|e| e.into_inner());
    played.retain(|(id, _)| id != entry_id);
    if played.len() >= PLAYED_MAX {
        played.remove(0);
    }
    played.push((entry_id.to_owned(), track.clone()));
    Some(track)
}

/// Titelwechsel, gemeldet vom DIENST statt von der Oberfläche.
///
/// Normalerweise führt die Flutter-Oberfläche die Warteschlange nach
/// (`queue_on_native_transition`). Läuft sie nicht – Musik im Hintergrund,
/// Android Auto, Widget –, würde die Warteschlange in Rust auf dem alten
/// Titel stehen bleiben: "Weiterspielen" nach einem Neustart begänne beim
/// falschen Titel, und beim Öffnen der App zeigte der Player Unsinn.
///
/// Rückgabe: `None`, wenn ExoPlayers Liste noch stimmt (der Normalfall),
/// sonst die neue Liste als JSON – etwa nach "Alle wiederholen" mit Shuffle,
/// wo die Runde neu gemischt wird.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn service_transition(entry_id: &str, wrapped: bool) -> Option<String> {
    let mut q = lock();
    let needs = q.sync_to_entry(entry_id, wrapped);
    persist(&q);
    // Neuer Titel -> die gemerkte Abspielposition gilt nicht mehr.
    let _ = db::set_meta(POSITION_KEY, "0");
    if needs {
        payload(&q, 0)
    } else {
        None
    }
}
