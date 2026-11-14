//! Bibliothek aus der LOKALEN Titel-Kopie: Alben, Künstler, Genres, Suche.
//!
//! WARUM lokal statt jedes Mal den Server zu fragen?
//!  - **Sofort da**: Die Bibliothek erscheint ohne Ladekreis.
//!  - **Offline nutzbar**: Alben und Künstler lassen sich auch ohne Netz
//!    durchblättern (abspielbar ist dann, was heruntergeladen ist).
//!  - **Flüssig bei großen Sammlungen**: Ruckeln und Wartezeiten bei großen
//!    Bibliotheken sind laut Nutzerkritik das häufigste Ärgernis solcher Apps.
//!
//! Die Kopie entsteht über `smart::sync_library`. Alben und Künstler werden
//! hier aus den Titeln abgeleitet (gruppiert), statt sie getrennt zu
//! speichern: So können sie nie von den Titeln abweichen.
//!
//! Die Titel-Liste wird einmal aus der Datenbank gelesen und dann im
//! Arbeitsspeicher gehalten, bis ein neuer Abgleich sie ersetzt. Bei 50.000
//! Titeln sind das wenige zehn MB – dafür muss nicht bei jedem Tippen JSON
//! neu geparst werden.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use rand::seq::SliceRandom;

use crate::api::models::{Album, Artist, SearchResult, Track};
use crate::core::db;

static CACHE: RwLock<Option<Arc<Vec<Track>>>> = RwLock::new(None);

/// Nach einem neuen Abgleich aufrufen, damit die nächste Abfrage frisch liest.
pub(crate) fn invalidate() {
    *CACHE.write().unwrap_or_else(|e| e.into_inner()) = None;
}

fn tracks() -> Result<Arc<Vec<Track>>> {
    if let Some(t) = CACHE.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return Ok(t);
    }
    let loaded = Arc::new(db::library_tracks()?);
    *CACHE.write().unwrap_or_else(|e| e.into_inner()) = Some(loaded.clone());
    Ok(loaded)
}

/// Schlüssel zum Sortieren von Namen: Kleinbuchstaben und ohne führenden
/// Artikel, damit "The Beatles" unter B steht und nicht unter T.
fn sort_key(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    for article in ["the ", "die ", "der ", "das "] {
        if let Some(rest) = lower.strip_prefix(article) {
            return rest.to_owned();
        }
    }
    lower
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSort {
    Name,
    Artist,
    /// Neueste Veröffentlichung zuerst.
    YearNewest,
    /// Zuletzt zur Bibliothek hinzugefügt zuerst.
    AddedNewest,
}

/// Baut aus Titeln die Alben. `added` = spätester Zeitpunkt, zu dem ein
/// Titel des Albums hinzukam (für "zuletzt hinzugefügt").
fn group_albums<'a>(tracks: impl Iterator<Item = &'a Track>) -> Vec<(Album, i64)> {
    let mut map: HashMap<&str, (Album, i64)> = HashMap::new();
    for t in tracks {
        let Some(album_id) = t.album_id.as_deref() else { continue };
        let entry = map.entry(album_id).or_insert_with(|| {
            (
                Album {
                    id: album_id.to_owned(),
                    name: t.album.clone().unwrap_or_default(),
                    artist: t.artist.clone(),
                    artist_id: t.artist_id.clone(),
                    cover_art_id: t.cover_art_id.clone(),
                    year: t.year,
                    song_count: Some(0),
                    duration_secs: Some(0),
                    starred: false,
                },
                0,
            )
        });
        let (album, added) = entry;
        album.song_count = Some(album.song_count.unwrap_or(0) + 1);
        album.duration_secs = Some(album.duration_secs.unwrap_or(0) + t.duration_secs.unwrap_or(0));
        if album.year.is_none() {
            album.year = t.year;
        }
        if album.cover_art_id.is_none() {
            album.cover_art_id = t.cover_art_id.clone();
        }
        // Verschiedene Künstler auf einem Album (Sampler): Dann ist kein
        // einzelner Name richtig, also keinen anzeigen.
        if album.artist != t.artist {
            album.artist = None;
            album.artist_id = None;
        }
        *added = (*added).max(t.added_at.unwrap_or(0));
    }
    map.into_values().collect()
}

fn sort_albums(mut albums: Vec<(Album, i64)>, sort: AlbumSort) -> Vec<Album> {
    match sort {
        AlbumSort::Name => albums.sort_by_cached_key(|(a, _)| sort_key(&a.name)),
        AlbumSort::Artist => albums.sort_by_cached_key(|(a, _)| {
            (sort_key(a.artist.as_deref().unwrap_or("")), a.year, sort_key(&a.name))
        }),
        AlbumSort::YearNewest => {
            albums.sort_by_cached_key(|(a, _)| (std::cmp::Reverse(a.year), sort_key(&a.name)))
        }
        AlbumSort::AddedNewest => {
            albums.sort_by_cached_key(|(a, added)| (std::cmp::Reverse(*added), sort_key(&a.name)))
        }
    }
    albums.into_iter().map(|(a, _)| a).collect()
}

/// Alle Alben der lokalen Kopie.
pub fn local_albums(sort: AlbumSort) -> Result<Vec<Album>> {
    let tracks = tracks()?;
    Ok(sort_albums(group_albums(tracks.iter()), sort))
}

/// Alle Künstler der lokalen Kopie, alphabetisch.
pub fn local_artists() -> Result<Vec<Artist>> {
    let tracks = tracks()?;
    // Künstler-ID -> (Künstler, Menge seiner Alben)
    let mut map: HashMap<&str, (Artist, std::collections::HashSet<&str>)> = HashMap::new();
    for t in tracks.iter() {
        let (Some(id), Some(name)) = (t.artist_id.as_deref(), t.artist.as_deref()) else {
            continue;
        };
        let entry = map.entry(id).or_insert_with(|| {
            (
                Artist {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    cover_art_id: t.cover_art_id.clone(),
                    album_count: None,
                    starred: false,
                },
                Default::default(),
            )
        });
        if let Some(album_id) = t.album_id.as_deref() {
            entry.1.insert(album_id);
        }
    }
    let mut artists: Vec<Artist> = map
        .into_values()
        .map(|(mut a, albums)| {
            a.album_count = Some(albums.len() as u32);
            a
        })
        .collect();
    artists.sort_by_cached_key(|a| sort_key(&a.name));
    Ok(artists)
}

/// Titel eines Albums in Album-Reihenfolge (CD, dann Titelnummer).
/// Damit lässt sich eine Album-Seite auch offline anzeigen.
pub fn local_album_tracks(album_id: String) -> Result<Vec<Track>> {
    let tracks = tracks()?;
    let mut out: Vec<Track> =
        tracks.iter().filter(|t| t.album_id.as_deref() == Some(&album_id)).cloned().collect();
    out.sort_by_key(|t| (t.disc_number.unwrap_or(1), t.track_number.unwrap_or(0)));
    Ok(out)
}

/// Alben eines Künstlers, neueste zuerst.
pub fn local_artist_albums(artist_id: String) -> Result<Vec<Album>> {
    let tracks = tracks()?;
    let albums = group_albums(tracks.iter().filter(|t| t.artist_id.as_deref() == Some(&artist_id)));
    Ok(sort_albums(albums, AlbumSort::YearNewest))
}

/// Ein Genre mit der Anzahl seiner Titel.
#[derive(Debug, Clone)]
pub struct GenreCount {
    pub name: String,
    pub track_count: u32,
}

/// Alle Genres, die häufigsten zuerst (für die Kacheln in der Suche).
pub fn local_genres() -> Result<Vec<GenreCount>> {
    let tracks = tracks()?;
    let mut map: HashMap<&str, u32> = HashMap::new();
    for t in tracks.iter() {
        if let Some(g) = t.genre.as_deref().filter(|g| !g.trim().is_empty()) {
            *map.entry(g).or_default() += 1;
        }
    }
    let mut out: Vec<GenreCount> =
        map.into_iter().map(|(name, track_count)| GenreCount { name: name.to_owned(), track_count }).collect();
    out.sort_by(|a, b| b.track_count.cmp(&a.track_count).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

/// Alle Titel eines Genres, nach Künstler und Album sortiert.
pub fn local_genre_tracks(genre: String) -> Result<Vec<Track>> {
    let tracks = tracks()?;
    let mut out: Vec<Track> =
        tracks.iter().filter(|t| t.genre.as_deref() == Some(&genre)).cloned().collect();
    out.sort_by_cached_key(|t| {
        (
            sort_key(t.artist.as_deref().unwrap_or("")),
            sort_key(t.album.as_deref().unwrap_or("")),
            t.disc_number,
            t.track_number,
        )
    });
    Ok(out)
}

/// Zufällige Titel aus der GANZEN Bibliothek ("Alles zufällig").
/// Andere Apps mischen oft nur die ersten paar hundert Titel; hier hat
/// jeder Titel die gleiche Chance.
pub fn local_random_tracks(count: u32) -> Result<Vec<Track>> {
    let tracks = tracks()?;
    let mut rng = rand::thread_rng();
    Ok(tracks.choose_multiple(&mut rng, count as usize).cloned().collect())
}

/// Sucht in der lokalen Kopie. Alle Suchwörter müssen vorkommen; die
/// Reihenfolge und Groß-/Kleinschreibung sind egal.
///
/// Ergebnisse, deren Name mit dem Suchtext BEGINNT, stehen oben – so
/// findet "love" zuerst "Love" und erst danach "Glove".
pub fn local_search(query: String) -> Result<SearchResult> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Ok(SearchResult::default());
    }
    let first = words[0].clone();
    let matches = |text: &str| {
        let lower = text.to_lowercase();
        words.iter().all(|w| lower.contains(w))
    };
    // 0 = beginnt mit dem Suchtext, 1 = enthält ihn nur.
    let rank = |name: &str| if name.to_lowercase().starts_with(&first) { 0u8 } else { 1u8 };

    let tracks = tracks()?;

    let mut hits: Vec<&Track> = tracks
        .iter()
        .filter(|t| {
            matches(&format!(
                "{} {} {}",
                t.title,
                t.artist.as_deref().unwrap_or(""),
                t.album.as_deref().unwrap_or("")
            ))
        })
        .collect();
    hits.sort_by_cached_key(|t| (rank(&t.title), sort_key(&t.title)));

    let mut albums: Vec<Album> = group_albums(tracks.iter())
        .into_iter()
        .map(|(a, _)| a)
        .filter(|a| matches(&format!("{} {}", a.name, a.artist.as_deref().unwrap_or(""))))
        .collect();
    albums.sort_by_cached_key(|a| (rank(&a.name), sort_key(&a.name)));

    let mut artists: Vec<Artist> = local_artists()?.into_iter().filter(|a| matches(&a.name)).collect();
    artists.sort_by_cached_key(|a| (rank(&a.name), sort_key(&a.name)));

    Ok(SearchResult {
        artists: artists.into_iter().take(10).collect(),
        albums: albums.into_iter().take(20).collect(),
        tracks: hits.into_iter().take(50).cloned().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, title: &str, artist: &str, album: &str, year: u32) -> Track {
        Track {
            id: id.into(),
            title: title.into(),
            artist: Some(artist.into()),
            artist_id: Some(format!("ar-{artist}")),
            album: Some(album.into()),
            album_id: Some(format!("al-{album}")),
            year: Some(year),
            duration_secs: Some(100),
            ..Default::default()
        }
    }

    #[test]
    fn groups_tracks_into_albums_and_sorts() {
        let tracks = vec![
            t("1", "A", "The Zebras", "Zoo", 2001),
            t("2", "B", "The Zebras", "Zoo", 2001),
            t("3", "C", "Anna", "Berg", 2020),
        ];
        let by_name = sort_albums(group_albums(tracks.iter()), AlbumSort::Name);
        assert_eq!(by_name.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Berg", "Zoo"]);
        assert_eq!(by_name[1].song_count, Some(2));
        assert_eq!(by_name[1].duration_secs, Some(200));

        let newest = sort_albums(group_albums(tracks.iter()), AlbumSort::YearNewest);
        assert_eq!(newest[0].name, "Berg");
    }

    #[test]
    fn sampler_album_has_no_single_artist() {
        let tracks = vec![t("1", "A", "Eins", "Mix", 2000), t("2", "B", "Zwei", "Mix", 2000)];
        let albums = group_albums(tracks.iter());
        assert_eq!(albums[0].0.artist, None);
    }

    #[test]
    fn article_is_ignored_when_sorting() {
        assert_eq!(sort_key("The Beatles"), "beatles");
        assert_eq!(sort_key("Die Ärzte"), "ärzte");
        assert_eq!(sort_key("Theater"), "theater");
    }
}
