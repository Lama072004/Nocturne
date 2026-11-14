//! Die Warteschlange nach dem Spotify-Modell.
//!
//! # Aufbau
//!
//! ```text
//!   history      current     user_queue                  context
//!  [ A  B  C ]    [ D ]     [ X  Y ]  <- "Zur Warteschlange"  [ E  F  G ]
//!   ^ schon gespielt          "Als Nächstes in der            "Als Nächstes von:
//!                              Warteschlange"                  <Album/Playlist>"
//! ```
//!
//! * `context` ist das Album bzw. die Playlist, die der Nutzer gestartet hat.
//! * `user_queue` sind die Titel, die der Nutzer per "Zur Warteschlange"
//!   hinzugefügt hat. Sie werden IMMER vor dem Rest des Kontexts gespielt.
//!   Genau das macht Substreamer falsch: dort landet "Zur Warteschlange" ganz am
//!   Ende hinter der Playlist.
//! * Wird ein neues Album/eine neue Playlist gestartet, bleibt `user_queue`
//!   erhalten (wie bei Spotify, laut Spezifikation so gewünscht).
//!
//! # Warum hier in Rust und nicht in Kotlin/ExoPlayer?
//!
//! ExoPlayer kennt nur eine flache Liste. Die zwei Bereiche, das Verschieben
//! zwischen ihnen und "Zurück" mit korrektem Wiedereinsortieren sind reine
//! Logik – die soll plattformneutral (später auch Desktop) und gut testbar
//! sein. ExoPlayer bekommt das Ergebnis von [`PlayQueue::flatten`].

use std::sync::atomic::{AtomicU64, Ordering};

use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use crate::api::models::{RepeatMode, Track};

/// Aus welchem Bereich ein Eintrag stammt. Wichtig für "Zurück": Ein Titel
/// wandert dorthin zurück, wo er herkam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
    Context,
}

/// Ein einzelner Platz in der Warteschlange.
///
/// `entry_id` ist eindeutig pro Platz, NICHT pro Song: Derselbe Song kann
/// zweimal eingereiht sein und muss trotzdem unterscheidbar bleiben (für
/// Verschieben, Entfernen und die Zuordnung zu ExoPlayer-Einträgen).
#[derive(Debug, Clone)]
pub struct QueueEntry {
    pub entry_id: String,
    pub track: Track,
    pub origin: Origin,
}

static NEXT_ENTRY_ID: AtomicU64 = AtomicU64::new(1);

impl QueueEntry {
    fn new(track: Track, origin: Origin) -> Self {
        // Ein einfacher Zähler reicht: Die IDs müssen nur innerhalb dieses
        // App-Laufs eindeutig sein.
        let n = NEXT_ENTRY_ID.fetch_add(1, Ordering::Relaxed);
        Self { entry_id: format!("e{n}"), track, origin }
    }
}

/// Ein Titel in der gespeicherten Warteschlange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEntry {
    pub track: Track,
    /// `true` = vom Nutzer eingereiht, `false` = aus Album/Playlist.
    pub user: bool,
}

/// Die Warteschlange in einer Form, die sich als Datei speichern lässt.
///
/// Eintrags-IDs werden NICHT gespeichert: Sie gelten nur innerhalb eines
/// App-Laufs und werden beim Laden neu vergeben.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedQueue {
    pub history: Vec<SavedEntry>,
    pub current: Option<SavedEntry>,
    pub user_queue: Vec<Track>,
    pub context: Vec<Track>,
    pub context_name: Option<String>,
    pub shuffle: bool,
    /// 0 = aus, 1 = alle, 2 = Titel.
    pub repeat: u8,
}

/// So viele bereits gespielte Titel werden höchstens mitgespeichert. Mehr
/// braucht "Zurück" nach einem Neustart nicht, und die Datei bleibt klein.
const SAVED_HISTORY: usize = 50;

#[derive(Debug, Default)]
pub struct PlayQueue {
    pub history: Vec<QueueEntry>,
    pub current: Option<QueueEntry>,
    pub user_queue: Vec<QueueEntry>,
    pub context: Vec<QueueEntry>,
    /// Anzeigename für "Als Nächstes von: …".
    pub context_name: Option<String>,
    pub shuffle: bool,
    pub repeat: RepeatMode,
    /// Eintrags-IDs des Kontexts in der URSPRÜNGLICHEN Reihenfolge. Nur
    /// gefüllt, solange Shuffle an ist. Damit lässt sich beim Ausschalten
    /// die echte Album-/Playlist-Reihenfolge wiederherstellen.
    unshuffled: Vec<String>,
}

impl PlayQueue {
    /// Abbild zum Speichern (siehe [`SavedQueue`]).
    pub fn to_saved(&self) -> SavedQueue {
        let saved = |e: &QueueEntry| SavedEntry {
            track: e.track.clone(),
            user: e.origin == Origin::User,
        };
        let skip = self.history.len().saturating_sub(SAVED_HISTORY);
        SavedQueue {
            history: self.history.iter().skip(skip).map(saved).collect(),
            current: self.current.as_ref().map(saved),
            user_queue: self.user_queue.iter().map(|e| e.track.clone()).collect(),
            context: self.context.iter().map(|e| e.track.clone()).collect(),
            context_name: self.context_name.clone(),
            shuffle: self.shuffle,
            repeat: match self.repeat {
                RepeatMode::Off => 0,
                RepeatMode::All => 1,
                RepeatMode::One => 2,
            },
        }
    }

    /// Baut die Warteschlange aus einer gespeicherten Fassung wieder auf.
    pub fn from_saved(s: SavedQueue) -> Self {
        let entry = |e: SavedEntry| {
            QueueEntry::new(e.track, if e.user { Origin::User } else { Origin::Context })
        };
        let context: Vec<QueueEntry> =
            s.context.into_iter().map(|t| QueueEntry::new(t, Origin::Context)).collect();
        Self {
            history: s.history.into_iter().map(entry).collect(),
            current: s.current.map(entry),
            user_queue: s.user_queue.into_iter().map(|t| QueueEntry::new(t, Origin::User)).collect(),
            // Die ursprüngliche Album-Reihenfolge ist nach einem Neustart
            // nicht mehr bekannt. Wir merken uns die geladene Reihenfolge:
            // "Shuffle aus" lässt sie dann einfach, wie sie ist.
            unshuffled: context.iter().map(|e| e.entry_id.clone()).collect(),
            context,
            context_name: s.context_name,
            shuffle: s.shuffle,
            repeat: match s.repeat {
                1 => RepeatMode::All,
                2 => RepeatMode::One,
                _ => RepeatMode::Off,
            },
        }
    }

    /// Sucht einen Eintrag anhand seiner ID (aktuell, Verlauf oder kommend).
    pub fn find_entry(&self, entry_id: &str) -> Option<&QueueEntry> {
        self.current
            .iter()
            .chain(self.history.iter())
            .chain(self.user_queue.iter())
            .chain(self.context.iter())
            .find(|e| e.entry_id == entry_id)
    }

    /// Startet ein Album/eine Playlist ab Titel `start`.
    ///
    /// Die Titel VOR `start` kommen in den Verlauf, damit "Zurück" wie bei
    /// Spotify zum vorherigen Titel des Albums springt. Der alte Verlauf wird
    /// dafür verworfen. Die eigene Warteschlange bleibt unangetastet.
    pub fn play_context(&mut self, tracks: Vec<Track>, start: usize, name: Option<String>) {
        if tracks.is_empty() {
            return;
        }
        let start = start.min(tracks.len() - 1);
        let mut entries: Vec<QueueEntry> =
            tracks.into_iter().map(|t| QueueEntry::new(t, Origin::Context)).collect();
        self.context_name = name;

        if self.shuffle {
            // Wie bei Spotify: Der angetippte Titel startet, ALLE anderen
            // (auch die davor) kommen gemischt danach. Einen Verlauf gibt es
            // dann nicht, "Zurück" hätte in gemischter Reihenfolge keinen Sinn.
            self.current = Some(entries.remove(start));
            self.history.clear();
            self.context = entries;
            self.apply_shuffle();
        } else {
            self.context = entries.split_off(start + 1);
            self.current = entries.pop();
            self.history = entries;
            self.unshuffled.clear();
        }
    }

    /// Merkt sich die aktuelle Reihenfolge des Kontexts und mischt ihn dann.
    fn apply_shuffle(&mut self) {
        self.unshuffled = self.context.iter().map(|e| e.entry_id.clone()).collect();
        self.context.shuffle(&mut rand::thread_rng());
    }

    /// Shuffle ein/aus. Betrifft nur den Kontext: Die eigene Warteschlange
    /// behält immer die Reihenfolge, in der der Nutzer sie gefüllt hat.
    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            self.apply_shuffle();
        } else {
            // Ursprüngliche Reihenfolge wiederherstellen. Titel, die
            // inzwischen gespielt oder entfernt wurden, fehlen in `context`
            // und fallen dadurch automatisch weg.
            let order = std::mem::take(&mut self.unshuffled);
            let pos = |id: &str| order.iter().position(|o| o == id).unwrap_or(usize::MAX);
            self.context.sort_by_key(|e| pos(&e.entry_id));
        }
    }

    pub fn set_repeat(&mut self, mode: RepeatMode) {
        self.repeat = mode;
    }

    /// "Alle wiederholen": Nach dem letzten Titel beginnt der Kontext von vorne.
    ///
    /// Titel aus der eigenen Warteschlange werden dabei NICHT wiederholt.
    /// Sie waren ein einmaliger Wunsch, kein Teil des Albums.
    fn wrap_around(&mut self) {
        let mut all: Vec<QueueEntry> = std::mem::take(&mut self.history);
        all.extend(self.current.take());
        if all.iter().any(|e| e.origin == Origin::Context) {
            all.retain(|e| e.origin == Origin::Context);
            self.current = Some(all.remove(0));
            self.context = all;
            if self.shuffle {
                self.apply_shuffle();
            }
        } else if !all.is_empty() {
            // Sonderfall: Es gibt gar keinen Kontext (alles wurde von Hand
            // eingereiht). Dann wiederholen wir eben diese Titel, statt die
            // Warteschlange zu verlieren und die Wiedergabe zu stoppen.
            self.current = Some(all.remove(0));
            self.user_queue = all;
        }
    }

    /// Fügt einen Titel an Position `index` der kommenden Titel ein
    /// (kombinierte Liste `user_queue ++ context`). Gebraucht für
    /// "Rückgängig" nach dem Entfernen aus der Warteschlange.
    pub fn insert_upcoming(&mut self, index: usize, track: Track, into_user: bool) {
        if self.current.is_none() {
            self.play_context(vec![track], 0, None);
            return;
        }
        if into_user {
            let at = index.min(self.user_queue.len());
            self.user_queue.insert(at, QueueEntry::new(track, Origin::User));
        } else {
            let at = index.saturating_sub(self.user_queue.len()).min(self.context.len());
            self.context.insert(at, QueueEntry::new(track, Origin::Context));
        }
    }

    /// Verwirft den Rest des Albums/der Playlist ("Quelle beenden"). Der
    /// laufende Titel und die eigene Warteschlange bleiben.
    pub fn clear_context(&mut self) {
        self.context.clear();
        self.unshuffled.clear();
    }

    /// "Zur Warteschlange": hinten an die EIGENE Warteschlange, also noch vor
    /// den Rest des Kontexts.
    pub fn add_to_queue(&mut self, tracks: Vec<Track>) {
        if self.current.is_none() {
            // Es läuft noch nichts -> die Titel werden einfach zum neuen
            // Kontext, sonst gäbe es keinen "aktuellen" Titel.
            self.play_context(tracks, 0, None);
            return;
        }
        self.user_queue
            .extend(tracks.into_iter().map(|t| QueueEntry::new(t, Origin::User)));
    }

    /// Zum nächsten Titel. Die eigene Warteschlange hat Vorrang.
    pub fn advance(&mut self) -> bool {
        let next = if !self.user_queue.is_empty() {
            Some(self.user_queue.remove(0))
        } else if !self.context.is_empty() {
            Some(self.context.remove(0))
        } else {
            None
        };
        match next {
            Some(n) => {
                if let Some(cur) = self.current.replace(n) {
                    self.history.push(cur);
                }
                true
            }
            None => false,
        }
    }

    /// Zum vorherigen Titel. Der aktuelle Titel wandert an den Anfang seines
    /// Ursprungsbereichs zurück – ein eingereihter Titel landet also wieder
    /// in der eigenen Warteschlange, nicht im Kontext.
    pub fn go_back(&mut self) -> bool {
        let Some(prev) = self.history.pop() else { return false };
        if let Some(cur) = self.current.replace(prev) {
            match cur.origin {
                Origin::User => self.user_queue.insert(0, cur),
                Origin::Context => self.context.insert(0, cur),
            }
        }
        true
    }

    /// Kommende Titel in Abspielreihenfolge: erst eigene Warteschlange, dann Kontext.
    pub fn upcoming_len(&self) -> usize {
        self.user_queue.len() + self.context.len()
    }

    /// Verschiebt einen kommenden Titel (Drag & Drop in der Warteschlange).
    ///
    /// `from` und `to` sind Positionen in der kombinierten Liste
    /// `user_queue ++ context`, `to` gemeint NACH dem Herausnehmen.
    ///
    /// Landet der Titel innerhalb der eigenen Warteschlange, gehört er danach
    /// zur eigenen Warteschlange, sonst zum Kontext. Genau auf der Grenze
    /// zwischen beiden Bereichen behält er seinen alten Bereich – so wirkt
    /// es in der UI natürlich ("ans Ende meiner Warteschlange" vs. "an den
    /// Anfang des Albums").
    pub fn move_upcoming(&mut self, from: usize, to: usize) -> bool {
        if from >= self.upcoming_len() {
            return false;
        }
        let mut entry = if from < self.user_queue.len() {
            self.user_queue.remove(from)
        } else {
            self.context.remove(from - self.user_queue.len())
        };

        let boundary = self.user_queue.len();
        let to = to.min(self.upcoming_len());
        let into_user = match to.cmp(&boundary) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => entry.origin == Origin::User,
        };

        if into_user {
            entry.origin = Origin::User;
            self.user_queue.insert(to, entry);
        } else {
            entry.origin = Origin::Context;
            self.context.insert(to - boundary, entry);
        }
        true
    }

    /// Entfernt einen kommenden Titel (z. B. per Wischgeste in der Warteschlange).
    pub fn remove_upcoming(&mut self, index: usize) -> bool {
        if index < self.user_queue.len() {
            self.user_queue.remove(index);
            true
        } else if index < self.upcoming_len() {
            self.context.remove(index - self.user_queue.len());
            true
        } else {
            false
        }
    }

    pub fn clear_user_queue(&mut self) {
        self.user_queue.clear();
    }

    /// Die flache Liste für ExoPlayer plus Index des aktuellen Titels.
    ///
    /// Reihenfolge: Verlauf, aktueller Titel, eigene Warteschlange, Kontext.
    /// Weil ExoPlayer genau in dieser Reihenfolge weiterspielt, stimmt die
    /// lückenlose Wiedergabe automatisch mit dem Spotify-Modell überein.
    pub fn flatten(&self) -> (Vec<&QueueEntry>, usize) {
        let mut out: Vec<&QueueEntry> = Vec::with_capacity(
            self.history.len() + 1 + self.upcoming_len(),
        );
        out.extend(self.history.iter());
        let index = out.len();
        out.extend(self.current.iter());
        out.extend(self.user_queue.iter());
        out.extend(self.context.iter());
        (out, index)
    }

    fn flat_ids(&self) -> Vec<String> {
        self.flatten().0.iter().map(|e| e.entry_id.clone()).collect()
    }

    /// ExoPlayer meldet: Jetzt läuft Eintrag `entry_id` (z. B. nach
    /// automatischem Titelwechsel oder Tippen auf "Weiter" in der
    /// Benachrichtigung). Wir ziehen unser Modell nach.
    ///
    /// Rückgabe `true`, wenn sich dabei die flache Reihenfolge geändert hat und
    /// ExoPlayer deshalb eine aktualisierte Liste braucht. Das passiert nur
    /// beim Zurückgehen, wenn ein Kontext-Titel hinter die eigene
    /// Warteschlange einsortiert wird.
    ///
    /// `wrapped` sagt, ob ExoPlayer am ENDE der Liste von selbst wieder nach
    /// vorne gesprungen ist ("Alle wiederholen"). Nur dann wird der Kontext
    /// neu aufgebaut. Drückt der Nutzer dagegen "Zurück" bis zum ersten
    /// Eintrag, ist das ein normales Zurückgehen – eingereihte Titel wandern
    /// dabei zurück in die eigene Warteschlange, statt verloren zu gehen.
    pub fn sync_to_entry(&mut self, entry_id: &str, wrapped: bool) -> bool {
        let before = self.flat_ids();
        let (_, cur_idx) = self.flatten();
        let Some(target) = before.iter().position(|id| id == entry_id) else {
            return false;
        };
        // ExoPlayer ist vom LETZTEN Eintrag auf den ERSTEN gesprungen: Das
        // passiert nur bei "Alle wiederholen" am Ende der Liste.
        if wrapped
            && self.repeat == RepeatMode::All
            && target == 0
            && cur_idx > 0
            && self.upcoming_len() == 0
        {
            self.wrap_around();
            // Nach dem Umbruch läuft evtl. ein anderer Eintrag als ExoPlayer
            // meint (übersprungene Warteschlangen-Titel, neu gemischt) ->
            // ExoPlayer bekommt immer die neue Liste.
            return true;
        }
        if target > cur_idx {
            for _ in 0..(target - cur_idx) {
                self.advance();
            }
        } else {
            for _ in 0..(cur_idx - target) {
                self.go_back();
            }
        }
        self.flat_ids() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str) -> Track {
        Track { id: id.into(), title: id.into(), ..Default::default() }
    }
    fn titles(v: &[QueueEntry]) -> Vec<&str> {
        v.iter().map(|e| e.track.id.as_str()).collect()
    }
    fn flat(q: &PlayQueue) -> Vec<String> {
        q.flatten().0.iter().map(|e| e.track.id.clone()).collect()
    }

    #[test]
    fn add_to_queue_plays_before_rest_of_context() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b"), t("c")], 0, None);
        q.add_to_queue(vec![t("x")]);
        q.add_to_queue(vec![t("y")]);
        // x und y vor b und c – und y hinter x (Reihenfolge des Hinzufügens).
        assert_eq!(flat(&q), ["a", "x", "y", "b", "c"]);
    }

    #[test]
    fn user_queue_survives_new_context() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b")], 0, None);
        q.add_to_queue(vec![t("x")]);
        q.play_context(vec![t("n1"), t("n2"), t("n3")], 1, Some("Neu".into()));
        assert_eq!(flat(&q), ["n1", "n2", "x", "n3"]);
        assert_eq!(q.flatten().1, 1);
    }

    #[test]
    fn going_back_returns_user_track_to_user_queue() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b")], 0, None);
        q.add_to_queue(vec![t("x")]);
        q.advance(); // spielt x
        assert_eq!(q.current.as_ref().unwrap().track.id, "x");
        q.go_back(); // zurück zu a, x wieder vorne in der eigenen Queue
        assert_eq!(titles(&q.user_queue), ["x"]);
        assert_eq!(flat(&q), ["a", "x", "b"]);
    }

    #[test]
    fn move_between_sections() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b"), t("c")], 0, None);
        q.add_to_queue(vec![t("x")]);
        // upcoming = [x | b c]; c (Index 2) ganz nach vorne -> wird eigener Eintrag.
        q.move_upcoming(2, 0);
        assert_eq!(titles(&q.user_queue), ["c", "x"]);
        assert_eq!(titles(&q.context), ["b"]);
        // x (Index 1) hinter b -> wird Kontext.
        q.move_upcoming(1, 2);
        assert_eq!(titles(&q.user_queue), ["c"]);
        assert_eq!(titles(&q.context), ["b", "x"]);
    }

    #[test]
    fn shuffle_keeps_tapped_track_and_can_be_undone() {
        let mut q = PlayQueue::default();
        q.set_shuffle(true);
        q.play_context(vec![t("a"), t("b"), t("c"), t("d")], 2, None);
        // Der angetippte Titel läuft, alle anderen kommen danach.
        assert_eq!(q.current.as_ref().unwrap().track.id, "c");
        let mut rest = titles(&q.context);
        rest.sort();
        assert_eq!(rest, ["a", "b", "d"]);
        // Ausschalten stellt die Album-Reihenfolge wieder her.
        q.set_shuffle(false);
        assert_eq!(titles(&q.context), ["a", "b", "d"]);
    }

    #[test]
    fn repeat_all_restarts_context_without_user_tracks() {
        let mut q = PlayQueue::default();
        q.set_repeat(RepeatMode::All);
        q.play_context(vec![t("a"), t("b")], 0, None);
        q.add_to_queue(vec![t("x")]);
        let first = q.flatten().0[0].entry_id.clone();
        q.advance(); // x
        q.advance(); // b (letzter Titel)
        // ExoPlayer springt am Ende zurück auf den ersten Eintrag.
        assert!(q.sync_to_entry(&first, true));
        assert_eq!(flat(&q), ["a", "b"]);
    }

    #[test]
    fn repeat_all_with_only_user_tracks_keeps_the_queue() {
        // Alles von Hand eingereiht, kein Kontext: Die Warteschlange darf
        // beim Umbruch nicht verschwinden.
        let mut q = PlayQueue::default();
        q.set_repeat(RepeatMode::All);
        q.play_context(vec![t("a")], 0, None);
        q.add_to_queue(vec![t("x"), t("y")]);
        q.context.clear();
        // "a" stammt aus dem Kontext; für den Test alles zu eigenen Titeln machen.
        q.current.as_mut().unwrap().origin = Origin::User;
        let first = q.flatten().0[0].entry_id.clone();
        q.advance();
        q.advance();
        assert!(q.sync_to_entry(&first, true));
        assert_eq!(flat(&q), ["a", "x", "y"]);
    }

    #[test]
    fn going_back_to_first_entry_is_not_a_wrap() {
        let mut q = PlayQueue::default();
        q.set_repeat(RepeatMode::All);
        q.play_context(vec![t("a"), t("b")], 0, None);
        q.add_to_queue(vec![t("x")]);
        let first = q.flatten().0[0].entry_id.clone();
        q.advance(); // x
        q.advance(); // b (letzter)
        // Der Nutzer geht von Hand zurück bis zum ersten Titel.
        q.sync_to_entry(&first, false);
        // x ist wieder in der eigenen Warteschlange, nichts ging verloren.
        assert_eq!(flat(&q), ["a", "x", "b"]);
    }

    #[test]
    fn insert_and_clear_context() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b"), t("c")], 0, None);
        q.add_to_queue(vec![t("x")]);
        q.insert_upcoming(0, t("u"), true);
        q.insert_upcoming(3, t("k"), false); // nach b
        assert_eq!(flat(&q), ["a", "u", "x", "b", "k", "c"]);
        q.clear_context();
        assert_eq!(flat(&q), ["a", "u", "x"]);
    }

    #[test]
    fn save_and_restore_keeps_order_and_sections() {
        let mut q = PlayQueue::default();
        q.set_repeat(RepeatMode::All);
        q.play_context(vec![t("a"), t("b"), t("c")], 1, Some("Album".into()));
        q.add_to_queue(vec![t("x")]);
        let restored = PlayQueue::from_saved(q.to_saved());
        assert_eq!(flat(&restored), ["a", "b", "x", "c"]);
        assert_eq!(restored.flatten().1, 1);
        assert_eq!(titles(&restored.user_queue), ["x"]);
        assert_eq!(restored.context_name.as_deref(), Some("Album"));
        assert_eq!(restored.repeat, RepeatMode::All);
        // Als JSON hin und zurück (so liegt es auf der Platte).
        let json = serde_json::to_string(&q.to_saved()).unwrap();
        let back = PlayQueue::from_saved(serde_json::from_str(&json).unwrap());
        assert_eq!(flat(&back), ["a", "b", "x", "c"]);
    }

    #[test]
    fn sync_forward_and_back() {
        let mut q = PlayQueue::default();
        q.play_context(vec![t("a"), t("b"), t("c")], 0, None);
        q.add_to_queue(vec![t("x")]);
        let ids: Vec<String> = q.flatten().0.iter().map(|e| e.entry_id.clone()).collect();
        // ExoPlayer springt zwei weiter (auf b).
        assert!(!q.sync_to_entry(&ids[2], false));
        assert_eq!(q.current.as_ref().unwrap().track.id, "b");
        // Und wieder zurück auf a: Reihenfolge bleibt gleich.
        assert!(!q.sync_to_entry(&ids[0], false));
        assert_eq!(flat(&q), ["a", "x", "b", "c"]);
    }
}
