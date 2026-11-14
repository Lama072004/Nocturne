//! Server-Adapter.
//!
//! WARUM ein `enum` statt eines Traits mit `dyn`?
//! Asynchrone Trait-Methoden lassen sich in Rust (noch) nicht bequem als
//! `dyn Trait` verwenden. Mit einem `enum` ist jeder Aufruf ein einfaches
//! `match`. Dazu kommt kein zusätzliches Crate (async-trait), und der
//! Compiler meldet sofort, wenn ein neuer Adapter eine Methode vergisst.
//!
//! Jeder Server bekommt eine Variante in [`ServerClient`]; die UI merkt davon
//! nichts, weil alle Adapter dieselben Typen aus `api::models` zurückgeben.
//!
//! * `subsonic`: Navidrome, Gonic, Airsonic, Ampache …
//! * `jellyfin`: Jellyfin UND Emby (fast gleiche API, ein Adapter mit
//!   [`jellyfin::Flavor`] für die Unterschiede)
//! * `plex`: Plex Media Server

pub mod jellyfin;
pub mod plex;
pub mod subsonic;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::api::models::{
    Album, AlbumDetail, Artist, ArtistDetail, Lyrics, Playlist, PlaylistDetail, SearchResult,
    ServerInfo, StarKind, Starred, Track,
};
use crate::api::settings::StreamQuality;

/// Was zum Wiederverbinden gespeichert wird. Enthält KEIN Klartext-Passwort.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum SavedSession {
    Subsonic(subsonic::SubsonicCredentials),
    Jellyfin(jellyfin::JellyfinCredentials),
    Emby(jellyfin::JellyfinCredentials),
    Plex(plex::PlexCredentials),
}

pub enum ServerClient {
    Subsonic(subsonic::SubsonicClient),
    /// Beide Varianten teilen sich den Client; `Flavor` steckt darin.
    Jellyfin(jellyfin::JellyfinClient),
    Emby(jellyfin::JellyfinClient),
    Plex(plex::PlexClient),
}

/// Reicht einen Aufruf an den jeweiligen Adapter weiter.
///
/// Das Makro spart das immer gleiche `match self { Self::Subsonic(c) => … }`
/// in jeder Methode. Kommt ein Server dazu, wird nur HIER eine Zeile ergänzt.
macro_rules! dispatch {
    ($self:ident, $c:ident => $call:expr) => {
        match $self {
            ServerClient::Subsonic($c) => $call,
            ServerClient::Jellyfin($c) | ServerClient::Emby($c) => $call,
            ServerClient::Plex($c) => $call,
        }
    };
}

impl ServerClient {
    pub fn from_saved(saved: SavedSession) -> Result<Self> {
        Ok(match saved {
            SavedSession::Subsonic(c) => Self::Subsonic(subsonic::SubsonicClient::new(c)?),
            SavedSession::Jellyfin(c) => {
                Self::Jellyfin(jellyfin::JellyfinClient::new(jellyfin::Flavor::Jellyfin, c)?)
            }
            SavedSession::Emby(c) => Self::Emby(jellyfin::JellyfinClient::new(jellyfin::Flavor::Emby, c)?),
            SavedSession::Plex(c) => Self::Plex(plex::PlexClient::new(c)?),
        })
    }

    pub fn saved(&self) -> SavedSession {
        match self {
            Self::Subsonic(c) => SavedSession::Subsonic(c.credentials().clone()),
            Self::Jellyfin(c) => SavedSession::Jellyfin(c.credentials().clone()),
            Self::Emby(c) => SavedSession::Emby(c.credentials().clone()),
            Self::Plex(c) => SavedSession::Plex(c.credentials().clone()),
        }
    }

    pub async fn ping(&self) -> Result<ServerInfo> {
        dispatch!(self, c => c.ping().await)
    }

    pub async fn newest_albums(&self, count: u32) -> Result<Vec<Album>> {
        dispatch!(self, c => c.newest_albums(count).await)
    }

    pub async fn recent_albums(&self, count: u32) -> Result<Vec<Album>> {
        dispatch!(self, c => c.recent_albums(count).await)
    }

    pub async fn albums_page(&self, offset: u32, size: u32) -> Result<Vec<Album>> {
        dispatch!(self, c => c.albums_page(offset, size).await)
    }

    pub async fn album(&self, id: &str) -> Result<AlbumDetail> {
        dispatch!(self, c => c.album(id).await)
    }

    pub async fn artists(&self) -> Result<Vec<Artist>> {
        dispatch!(self, c => c.artists().await)
    }

    pub async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        dispatch!(self, c => c.artist(id).await)
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        dispatch!(self, c => c.playlists().await)
    }

    pub async fn playlist(&self, id: &str) -> Result<PlaylistDetail> {
        dispatch!(self, c => c.playlist(id).await)
    }

    pub async fn create_playlist(&self, name: &str, song_ids: &[String]) -> Result<()> {
        dispatch!(self, c => c.create_playlist(name, song_ids).await)
    }

    pub async fn add_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        dispatch!(self, c => c.add_to_playlist(playlist_id, song_ids).await)
    }

    pub async fn starred(&self) -> Result<Starred> {
        dispatch!(self, c => c.starred().await)
    }

    pub async fn set_starred(&self, kind: StarKind, id: &str, starred: bool) -> Result<()> {
        dispatch!(self, c => c.set_starred(kind, id, starred).await)
    }

    pub async fn scrobble(&self, song_id: &str, time_ms: Option<i64>, submission: bool) -> Result<()> {
        dispatch!(self, c => c.scrobble(song_id, time_ms, submission).await)
    }

    pub async fn lyrics(&self, song_id: &str) -> Result<Option<Lyrics>> {
        dispatch!(self, c => c.lyrics(song_id).await)
    }

    pub async fn songs_page(&self, offset: u32, size: u32) -> Result<Vec<Track>> {
        dispatch!(self, c => c.songs_page(offset, size).await)
    }

    pub async fn write_playlist(
        &self,
        existing_id: Option<&str>,
        name: &str,
        song_ids: &[String],
    ) -> Result<Option<String>> {
        dispatch!(self, c => c.write_playlist(existing_id, name, song_ids).await)
    }

    pub async fn search(&self, query: &str) -> Result<SearchResult> {
        dispatch!(self, c => c.search(query).await)
    }

    pub fn cover_url(&self, cover_id: &str, size: u32) -> String {
        dispatch!(self, c => c.cover_url(cover_id, size))
    }

    pub fn stream_url(&self, song_id: &str, quality: StreamQuality) -> String {
        dispatch!(self, c => c.stream_url(song_id, quality))
    }
}
