//! Inhaltsbaum für Android Auto, Sprachassistenten und Widgets.
//!
//! Diese Oberflächen zeigen keine Flutter-Seiten. Sie fragen den
//! Android-Dienst: "Was liegt unter Knoten X?" und "Spiele Eintrag Y". Der
//! Dienst reicht beide Fragen per JNI hierher weiter. So gibt es die Logik
//! (welche Listen, in welcher Reihenfolge, was wird abgespielt) nur einmal –
//! in Rust, wo auch die Flutter-Oberfläche sie benutzt.
//!
//! # Aufbau des Baums
//!
//! ```text
//! root
//! ├── recent      Zuletzt gehört      -> Alben
//! ├── playlists   Playlists           -> Playlists und Smart-Playlists
//! ├── albums      Alben               -> Alben (aus der lokalen Kopie)
//! └── downloads   Heruntergeladen     -> Titel
//! ```
//!
//! Höchstens vier Einträge auf der obersten Ebene und höchstens drei Ebenen
//! tief: Das sind die Vorgaben von Android Auto, damit man beim Fahren
//! nicht lange suchen muss.
//!
//! # IDs
//! Jeder Eintrag hat eine Text-ID, aus der sich ablesen lässt, was er ist:
//! `album:<id>`, `playlist:<id>`, `smart:<id>` (aufklappbar) und
//! `track:<elternknoten>#<index>` (abspielbar). Ein Titel trägt also seine
//! Liste und Position in sich: Beim Abspielen wird die GANZE Liste zur
//! Warteschlange, beginnend bei diesem Titel – wie beim Antippen in der App.

use std::collections::HashMap;
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use serde::Serialize;

use crate::api::library;
use crate::api::local::{self, AlbumSort};
use crate::api::models::{Album, Track};
use crate::core::{db, runtime};

/// Ein Eintrag im Baum, so wie ihn Kotlin als JSON bekommt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseItem {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub cover_id: Option<String>,
    /// Aufklappbar (Ordner, Album, Playlist).
    pub browsable: bool,
    /// Direkt abspielbar (Titel).
    pub playable: bool,
}

pub const ROOT: &str = "root";
const RECENT: &str = "recent";
const PLAYLISTS: &str = "playlists";
const ALBUMS: &str = "albums";
const DOWNLOADS: &str = "downloads";
/// Ergebnisse der letzten Sprachsuche.
const SEARCH: &str = "search";

/// Höchstzahl an Einträgen pro Liste. Auto-Oberflächen kürzen ohnehin, und
/// lange Listen sind beim Fahren nicht bedienbar.
const MAX_ITEMS: usize = 300;

/// Zuletzt gelieferte Titel-Listen je Knoten. Damit muss beim Abspielen
/// (`track:<knoten>#<index>`) nicht noch einmal der Server gefragt werden –
/// im Auto zählt, dass die Musik sofort startet.
static LISTS: Mutex<Option<HashMap<String, Vec<Track>>>> = Mutex::new(None);

fn remember(node: &str, tracks: Vec<Track>) {
    let mut guard = LISTS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    // Nicht unbegrenzt wachsen lassen: Die Listen sind nur ein Kurzzeit-Merker.
    if map.len() > 30 {
        map.clear();
    }
    map.insert(node.to_owned(), tracks);
}

fn recall(node: &str) -> Option<Vec<Track>> {
    LISTS.lock().unwrap_or_else(|e| e.into_inner()).as_ref()?.get(node).cloned()
}

/// Anzeigenamen der zuletzt gelieferten Alben und Playlists (ID -> Titel).
///
/// WARUM? Startet jemand ein Album über ein Widget oder Android Auto, soll
/// der Player in der App danach "Wiedergabe von: <Album>" zeigen – genau wie
/// beim Start aus der App. Beim Abspielen kennen wir aber nur die ID; den
/// Namen haben wir kurz vorher beim Auflisten gesehen.
static NAMES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn remember_names(items: &[BrowseItem]) {
    let mut guard = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    // Kurzzeit-Merker wie LISTS: nicht unbegrenzt wachsen lassen.
    if map.len() > 2000 {
        map.clear();
    }
    for item in items.iter().filter(|i| i.browsable) {
        map.insert(item.id.clone(), item.title.clone());
    }
}

/// Name der Liste hinter `node`, soweit bekannt.
fn list_name(node: &str, tracks: &[Track]) -> Option<String> {
    let known = NAMES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get(node).cloned());
    if known.is_some() {
        return known;
    }
    // Ein Album verrät seinen Namen auch über seine Titel (z. B. nach einem
    // Neustart, wenn der Merker leer ist).
    if node.starts_with("album:") {
        return tracks.first().and_then(|t| t.album.clone());
    }
    None
}

fn folder(id: &str, title: &str) -> BrowseItem {
    BrowseItem {
        id: id.to_owned(),
        title: title.to_owned(),
        subtitle: None,
        cover_id: None,
        browsable: true,
        playable: false,
    }
}

fn album_item(a: &Album) -> BrowseItem {
    BrowseItem {
        id: format!("album:{}", a.id),
        title: a.name.clone(),
        subtitle: a.artist.clone(),
        cover_id: a.cover_art_id.clone(),
        browsable: true,
        playable: false,
    }
}

fn track_items(node: &str, tracks: &[Track]) -> Vec<BrowseItem> {
    tracks
        .iter()
        .take(MAX_ITEMS)
        .enumerate()
        .map(|(i, t)| BrowseItem {
            id: format!("track:{node}#{i}"),
            title: t.title.clone(),
            subtitle: t.artist.clone(),
            cover_id: t.cover_art_id.clone(),
            browsable: false,
            playable: true,
        })
        .collect()
}

/// Titel einer aufklappbaren Liste (Album, Playlist, Smart-Playlist,
/// Downloads). Server zuerst, bei Fehlern die lokale Kopie.
fn tracks_of(node: &str) -> Result<Vec<Track>> {
    if node == DOWNLOADS {
        return db::downloads();
    }
    if let Some(id) = node.strip_prefix("album:") {
        if let Ok(client) = library::client() {
            if let Ok(detail) = runtime::block_on_net(client.album(id)) {
                return Ok(detail.tracks);
            }
        }
        return local::local_album_tracks(id.to_owned());
    }
    if let Some(id) = node.strip_prefix("playlist:") {
        let client = library::client()?;
        return Ok(runtime::block_on_net(client.playlist(id))?.tracks);
    }
    if let Some(id) = node.strip_prefix("smart:") {
        return crate::api::smart::evaluate_smart_playlist(id.to_owned());
    }
    bail!("Unbekannter Eintrag: {node}")
}

/// Kinder eines Knotens. `german` wählt die Sprache der festen Ordnernamen.
pub fn children(parent: &str, german: bool) -> Result<Vec<BrowseItem>> {
    let items = children_inner(parent, german)?;
    remember_names(&items);
    Ok(items)
}

fn children_inner(parent: &str, german: bool) -> Result<Vec<BrowseItem>> {
    let name = |de: &str, en: &str| if german { de.to_owned() } else { en.to_owned() };
    match parent {
        ROOT => Ok(vec![
            folder(RECENT, &name("Zuletzt gehört", "Recently played")),
            folder(PLAYLISTS, &name("Playlists", "Playlists")),
            folder(ALBUMS, &name("Alben", "Albums")),
            folder(DOWNLOADS, &name("Heruntergeladen", "Downloaded")),
        ]),
        RECENT => {
            // Online: was der Server als zuletzt gehört kennt (auch von
            // anderen Geräten). Offline: der eigene Verlauf der App.
            if let Ok(client) = library::client() {
                if let Ok(albums) = runtime::block_on_net(client.recent_albums(30)) {
                    if !albums.is_empty() {
                        return Ok(albums.iter().map(album_item).collect());
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            Ok(db::recent_plays(200)?
                .into_iter()
                .filter_map(|(t, _)| {
                    let id = t.album_id.clone()?;
                    seen.insert(id.clone()).then(|| BrowseItem {
                        id: format!("album:{id}"),
                        title: t.album.clone().unwrap_or_default(),
                        subtitle: t.artist.clone(),
                        cover_id: t.cover_art_id.clone(),
                        browsable: true,
                        playable: false,
                    })
                })
                .take(30)
                .collect())
        }
        PLAYLISTS => {
            let mut out: Vec<BrowseItem> = Vec::new();
            // Smart-Playlists zuerst: Sie liegen lokal und gehen immer.
            for p in db::smart_playlists()? {
                out.push(BrowseItem {
                    id: format!("smart:{}", p.id),
                    title: p.name,
                    subtitle: None,
                    cover_id: None,
                    browsable: true,
                    playable: false,
                });
            }
            if let Ok(client) = library::client() {
                if let Ok(lists) = runtime::block_on_net(client.playlists()) {
                    for p in lists {
                        out.push(BrowseItem {
                            id: format!("playlist:{}", p.id),
                            title: p.name,
                            subtitle: None,
                            cover_id: p.cover_art_id,
                            browsable: true,
                            playable: false,
                        });
                    }
                }
            }
            out.truncate(MAX_ITEMS);
            Ok(out)
        }
        ALBUMS => Ok(local::local_albums(AlbumSort::Name)?
            .iter()
            .take(MAX_ITEMS)
            .map(album_item)
            .collect()),
        SEARCH => Ok(track_items(SEARCH, &recall(SEARCH).unwrap_or_default())),
        node => {
            let tracks = tracks_of(node)?;
            let items = track_items(node, &tracks);
            remember(node, tracks);
            Ok(items)
        }
    }
}

/// Sprachsuche ("Spiele … auf Nocturne"). Liefert abspielbare Titel und
/// merkt sie sich unter dem Knoten `search`.
///
/// Erst die lokale Kopie (sofort, geht offline), dann der Server, falls
/// lokal nichts gefunden wurde. Eine leere Suche ("spiel Musik") liefert
/// zufällige Titel aus der ganzen Bibliothek.
pub fn search(query: &str) -> Result<Vec<BrowseItem>> {
    let query = query.trim();
    let mut tracks = if query.is_empty() {
        local::local_random_tracks(50)?
    } else {
        local::local_search(query.to_owned())?.tracks
    };
    if tracks.is_empty() && !query.is_empty() {
        if let Ok(client) = library::client() {
            if let Ok(result) = runtime::block_on_net(client.search(query)) {
                tracks = result.tracks;
            }
        }
    }
    let items = track_items(SEARCH, &tracks);
    remember(SEARCH, tracks);
    Ok(items)
}

/// Löst eine abspielbare ID in "diese Liste ab diesem Titel" auf.
///
/// Auch aufklappbare IDs sind erlaubt (`album:<id>` usw.): Dann wird die
/// Liste von vorne gespielt. Das braucht z. B. das Schnellzugriff-Widget.
pub fn resolve(media_id: &str) -> Result<(Vec<Track>, usize, Option<String>)> {
    let (node, index) = match media_id.strip_prefix("track:") {
        Some(rest) => {
            let (node, index) = rest.rsplit_once('#').ok_or_else(|| anyhow!("Ungültige ID"))?;
            (node, index.parse::<usize>().unwrap_or(0))
        }
        None => (media_id, 0),
    };
    let tracks = match recall(node) {
        Some(t) => t,
        None => tracks_of(node)?,
    };
    if tracks.is_empty() {
        bail!("Die Liste ist leer");
    }
    let name = list_name(node, &tracks);
    Ok((tracks, index, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str) -> Track {
        Track { id: id.into(), title: format!("Titel {id}"), ..Default::default() }
    }

    #[test]
    fn root_has_at_most_four_folders() {
        // Vorgabe von Android Auto.
        let root = children(ROOT, true).unwrap();
        assert_eq!(root.len(), 4);
        assert!(root.iter().all(|i| i.browsable && !i.playable));
        assert_eq!(children(ROOT, false).unwrap()[0].title, "Recently played");
    }

    #[test]
    fn track_ids_carry_list_and_position() {
        remember("album:x", vec![t("a"), t("b"), t("c")]);
        let items = track_items("album:x", &recall("album:x").unwrap());
        assert_eq!(items[1].id, "track:album:x#1");
        assert!(items[1].playable);

        // Abspielen von "b": ganze Liste, Start bei Index 1.
        let (tracks, index, _) = resolve("track:album:x#1").unwrap();
        assert_eq!(tracks.len(), 3);
        assert_eq!(index, 1);

        // Aufklappbare ID direkt abspielen = von vorne.
        let (_, index, _) = resolve("album:x").unwrap();
        assert_eq!(index, 0);
    }

    #[test]
    fn played_list_carries_its_name() {
        // Name aus den Titeln (Album), wenn die Liste nie aufgelistet wurde.
        let mut a = t("a");
        a.album = Some("Mein Album".into());
        remember("album:n1", vec![a]);
        assert_eq!(resolve("album:n1").unwrap().2.as_deref(), Some("Mein Album"));

        // Name aus der Auflistung (z. B. Playlist) hat Vorrang.
        remember_names(&[BrowseItem {
            id: "playlist:p1".into(),
            title: "Meine Playlist".into(),
            subtitle: None,
            cover_id: None,
            browsable: true,
            playable: false,
        }]);
        remember("playlist:p1", vec![t("x")]);
        assert_eq!(resolve("track:playlist:p1#0").unwrap().2.as_deref(), Some("Meine Playlist"));
    }
}
