//! Adapter für die Subsonic-API (Navidrome, Gonic, Airsonic, Ampache, …).
//!
//! Doku: <http://www.subsonic.org/pages/api.jsp> und <https://opensubsonic.netlify.app/>
//!
//! # Anmeldung
//! Subsonic nutzt "Token-Auth": `t = md5(passwort + salt)`, `s = salt`.
//! Das Passwort selbst geht nie über das Netz. Wir speichern auch nur Token
//! und Salt, nicht das Passwort.
//! Achtung: Das Token ist trotzdem passwortgleich (wer es hat, kommt rein).
//! TODO: später im Android-Keystore verschlüsseln.
//!
//! Nextcloud Music und Ampache brauchen stattdessen "Legacy-Auth" mit einem
//! API-Key. Das kommt als eigene Option im Login dazu.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use md5::{Digest, Md5};
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::api::models::{
    Album, AlbumDetail, Artist, ArtistDetail, LyricLine, Lyrics, Playlist, PlaylistDetail,
    ReplayGainInfo, SearchResult, ServerInfo, ServerKind, StarKind, Starred, Track,
};
use crate::api::settings::{AudioFormat, StreamQuality};

/// Höchste API-Version, die wir sprechen. Navidrome versteht 1.16.1.
const API_VERSION: &str = "1.16.1";
/// Name, unter dem die App beim Server auftaucht (z. B. in Navidromes "Player"-Liste).
const CLIENT_NAME: &str = "Nocturne";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsonicCredentials {
    /// Basis-URL ohne `/rest`, z. B. `https://music.example.org`
    pub base_url: String,
    pub username: String,
    pub token: String,
    pub salt: String,
}

impl SubsonicCredentials {
    /// Erzeugt Token + Salt aus dem Passwort. Das Passwort wird danach verworfen.
    pub fn from_password(base_url: &str, username: &str, password: &str) -> Self {
        // Neues zufälliges Salt bei jeder Anmeldung, so wie es die API empfiehlt.
        let salt = Alphanumeric.sample_string(&mut rand::thread_rng(), 12);
        Self {
            base_url: normalize_base_url(base_url),
            username: username.to_owned(),
            token: make_token(password, &salt),
            salt,
        }
    }
}

/// Wandelt einen Zeitstempel wie "2024-01-01T12:00:00Z" in Unix-Sekunden um.
///
/// Bewusst von Hand statt mit einem Datums-Crate: Wir brauchen nur dieses
/// eine feste Format und nur tagesgenaue Vergleiche ("vor 30 Tagen
/// hinzugefügt"). Zeitzonen-Angaben am Ende werden deshalb ignoriert.
pub(crate) fn parse_iso_utc(s: &str) -> Option<i64> {
    let num = |from: usize, to: usize| s.get(from..to)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, min, sec) = (num(11, 13).unwrap_or(0), num(14, 16).unwrap_or(0), num(17, 19).unwrap_or(0));
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Tage seit 1970-01-01 (Standard-Formel "days from civil").
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + min * 60 + sec)
}

fn make_token(password: &str, salt: &str) -> String {
    let mut h = Md5::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    hex::encode(h.finalize())
}

/// Nutzer tippen URLs sehr unterschiedlich ein. Wir machen daraus immer
/// `https://host[:port][/pfad]` ohne abschließenden Schrägstrich.
fn normalize_base_url(input: &str) -> String {
    let mut s = input.trim().trim_end_matches('/').to_owned();
    if !s.starts_with("http://") && !s.starts_with("https://") {
        // HTTPS als Standard: Bei HTTP gehen Token und Musik im Klartext übers Netz.
        s = format!("https://{s}");
    }
    if let Some(stripped) = s.strip_suffix("/rest") {
        s = stripped.to_owned();
    }
    s
}

pub struct SubsonicClient {
    creds: SubsonicCredentials,
    http: reqwest::Client,
}

impl SubsonicClient {
    pub fn new(creds: SubsonicCredentials) -> Result<Self> {
        let http = reqwest::Client::builder()
            // Lieber früh einen klaren Fehler anzeigen, als ewig zu hängen,
            // wenn der Server zu Hause nicht erreichbar ist.
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            // gzip wird nicht aktiviert, die JSON-Antworten sind klein; das
            // spart ein weiteres Crate in der APK.
            .build()?;
        Ok(Self { creds, http })
    }

    pub fn credentials(&self) -> &SubsonicCredentials {
        &self.creds
    }

    /// Baut eine vollständige URL inkl. Anmeldedaten.
    ///
    /// Subsonic erwartet die Anmeldung als Query-Parameter in JEDER Anfrage.
    /// Das gilt auch für Stream- und Cover-URLs, die ExoPlayer bzw. Flutter
    /// selbst laden. Deshalb bauen wir fertige URLs, statt Header zu setzen.
    fn url(&self, endpoint: &str, params: &[(&str, &str)]) -> String {
        let mut url = Url::parse(&format!("{}/rest/{endpoint}", self.creds.base_url))
            .unwrap_or_else(|_| Url::parse("https://invalid.invalid").unwrap());
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("u", &self.creds.username)
                .append_pair("t", &self.creds.token)
                .append_pair("s", &self.creds.salt)
                .append_pair("v", API_VERSION)
                .append_pair("c", CLIENT_NAME)
                .append_pair("f", "json");
            for (k, v) in params {
                q.append_pair(k, v);
            }
        }
        url.into()
    }

    /// Führt eine Anfrage aus und packt die `subsonic-response`-Hülle aus.
    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        endpoint: &str,
        params: &[(&str, &str)],
    ) -> Result<Envelope<T>> {
        let body = self
            .http
            .get(self.url(endpoint, params))
            .send()
            .await
            .with_context(|| format!("Server nicht erreichbar ({endpoint})"))?
            .error_for_status()?
            .bytes()
            .await?;
        parse_envelope(&body)
    }

    pub async fn ping(&self) -> Result<ServerInfo> {
        let r: Envelope<serde_json::Value> = self.get("ping", &[]).await?;
        Ok(ServerInfo {
            kind: ServerKind::Subsonic,
            server_type: r.server_type,
            server_version: r.server_version,
            api_version: r.version,
            open_subsonic: r.open_subsonic,
        })
    }

    /// Zuletzt hinzugefügte Alben.
    pub async fn newest_albums(&self, count: u32) -> Result<Vec<Album>> {
        self.album_list("newest", count).await
    }

    /// Zuletzt gehörte Alben.
    pub async fn recent_albums(&self, count: u32) -> Result<Vec<Album>> {
        self.album_list("recent", count).await
    }

    /// `type` z. B. "newest" (neu hinzugefügt) oder "recent" (zuletzt gehört).
    pub async fn album_list(&self, list_type: &str, count: u32) -> Result<Vec<Album>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "albumList2", default)]
            list: AlbumList,
        }
        #[derive(Deserialize, Default)]
        struct AlbumList {
            #[serde(default)]
            album: Vec<SAlbum>,
        }
        let count = count.to_string();
        let r: Envelope<R> =
            self.get("getAlbumList2", &[("type", list_type), ("size", &count)]).await?;
        Ok(r.body.list.album.into_iter().map(Into::into).collect())
    }

    pub async fn album(&self, id: &str) -> Result<AlbumDetail> {
        #[derive(Deserialize)]
        struct R {
            album: SAlbumWithSongs,
        }
        let r: Envelope<R> = self.get("getAlbum", &[("id", id)]).await?;
        let a = r.body.album;
        Ok(AlbumDetail {
            album: a.album.into(),
            tracks: a.song.into_iter().map(Into::into).collect(),
        })
    }

    pub async fn search(&self, query: &str) -> Result<SearchResult> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "searchResult3", default)]
            result: SSearch,
        }
        #[derive(Deserialize, Default)]
        struct SSearch {
            #[serde(default)]
            artist: Vec<SArtist>,
            #[serde(default)]
            album: Vec<SAlbum>,
            #[serde(default)]
            song: Vec<SSong>,
        }
        let r: Envelope<R> = self
            .get(
                "search3",
                &[("query", query), ("artistCount", "10"), ("albumCount", "20"), ("songCount", "50")],
            )
            .await?;
        Ok(SearchResult {
            artists: r.body.result.artist.into_iter().map(Into::into).collect(),
            albums: r.body.result.album.into_iter().map(Into::into).collect(),
            tracks: r.body.result.song.into_iter().map(Into::into).collect(),
        })
    }

    /// Alle Alben alphabetisch, seitenweise (für den Bibliothek-Tab).
    pub async fn albums_page(&self, offset: u32, size: u32) -> Result<Vec<Album>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "albumList2", default)]
            list: AlbumList,
        }
        #[derive(Deserialize, Default)]
        struct AlbumList {
            #[serde(default)]
            album: Vec<SAlbum>,
        }
        let (offset, size) = (offset.to_string(), size.to_string());
        let r: Envelope<R> = self
            .get(
                "getAlbumList2",
                &[("type", "alphabeticalByName"), ("size", &size), ("offset", &offset)],
            )
            .await?;
        Ok(r.body.list.album.into_iter().map(Into::into).collect())
    }

    /// Alle Künstler. Subsonic liefert sie nach Anfangsbuchstaben gruppiert;
    /// wir machen daraus eine flache, bereits sortierte Liste.
    pub async fn artists(&self) -> Result<Vec<Artist>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            artists: Indexes,
        }
        #[derive(Deserialize, Default)]
        struct Indexes {
            #[serde(default)]
            index: Vec<Index>,
        }
        #[derive(Deserialize)]
        struct Index {
            #[serde(default)]
            artist: Vec<SArtist>,
        }
        let r: Envelope<R> = self.get("getArtists", &[]).await?;
        Ok(r.body.artists.index.into_iter().flat_map(|i| i.artist).map(Into::into).collect())
    }

    pub async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        #[derive(Deserialize)]
        struct R {
            artist: SArtistWithAlbums,
        }
        #[derive(Deserialize)]
        struct SArtistWithAlbums {
            #[serde(flatten)]
            artist: SArtist,
            #[serde(default)]
            album: Vec<SAlbum>,
        }
        let r: Envelope<R> = self.get("getArtist", &[("id", id)]).await?;
        let a = r.body.artist;
        Ok(ArtistDetail {
            artist: a.artist.into(),
            albums: a.album.into_iter().map(Into::into).collect(),
        })
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(default)]
            playlists: Wrapper,
        }
        #[derive(Deserialize, Default)]
        struct Wrapper {
            #[serde(default)]
            playlist: Vec<SPlaylist>,
        }
        let r: Envelope<R> = self.get("getPlaylists", &[]).await?;
        Ok(r.body.playlists.playlist.into_iter().map(Into::into).collect())
    }

    pub async fn playlist(&self, id: &str) -> Result<PlaylistDetail> {
        #[derive(Deserialize)]
        struct R {
            playlist: SPlaylistWithSongs,
        }
        #[derive(Deserialize)]
        struct SPlaylistWithSongs {
            #[serde(flatten)]
            playlist: SPlaylist,
            #[serde(default)]
            entry: Vec<SSong>,
        }
        let r: Envelope<R> = self.get("getPlaylist", &[("id", id)]).await?;
        let p = r.body.playlist;
        Ok(PlaylistDetail {
            playlist: p.playlist.into(),
            tracks: p.entry.into_iter().map(Into::into).collect(),
        })
    }

    /// Legt eine neue Playlist mit den angegebenen Titeln an.
    pub async fn create_playlist(&self, name: &str, song_ids: &[String]) -> Result<()> {
        let mut params: Vec<(&str, &str)> = vec![("name", name)];
        // Subsonic erwartet denselben Parameter mehrfach: songId=1&songId=2 …
        params.extend(song_ids.iter().map(|id| ("songId", id.as_str())));
        let _: Envelope<serde_json::Value> = self.get("createPlaylist", &params).await?;
        Ok(())
    }

    pub async fn add_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        let mut params: Vec<(&str, &str)> = vec![("playlistId", playlist_id)];
        params.extend(song_ids.iter().map(|id| ("songIdToAdd", id.as_str())));
        let _: Envelope<serde_json::Value> = self.get("updatePlaylist", &params).await?;
        Ok(())
    }

    pub async fn starred(&self) -> Result<Starred> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "starred2", default)]
            starred: S,
        }
        #[derive(Deserialize, Default)]
        struct S {
            #[serde(default)]
            artist: Vec<SArtist>,
            #[serde(default)]
            album: Vec<SAlbum>,
            #[serde(default)]
            song: Vec<SSong>,
        }
        let r: Envelope<R> = self.get("getStarred2", &[]).await?;
        Ok(Starred {
            artists: r.body.starred.artist.into_iter().map(Into::into).collect(),
            albums: r.body.starred.album.into_iter().map(Into::into).collect(),
            tracks: r.body.starred.song.into_iter().map(Into::into).collect(),
        })
    }

    /// Favorit setzen oder entfernen.
    pub async fn set_starred(&self, kind: StarKind, id: &str, starred: bool) -> Result<()> {
        let param = match kind {
            StarKind::Track => "id",
            StarKind::Album => "albumId",
            StarKind::Artist => "artistId",
        };
        let endpoint = if starred { "star" } else { "unstar" };
        let _: Envelope<serde_json::Value> = self.get(endpoint, &[(param, id)]).await?;
        Ok(())
    }

    /// Meldet dem Server eine Wiedergabe.
    ///
    /// `submission = false` heißt "läuft gerade" (für die Anzeige am Server),
    /// `true` heißt "wurde gehört" (zählt als Wiedergabe). `time_ms` ist der
    /// Zeitpunkt des Hörens, damit offline Gehörtes später mit dem richtigen
    /// Datum nachgereicht werden kann.
    pub async fn scrobble(&self, song_id: &str, time_ms: Option<i64>, submission: bool) -> Result<()> {
        let time = time_ms.map(|t| t.to_string());
        let mut params: Vec<(&str, &str)> =
            vec![("id", song_id), ("submission", if submission { "true" } else { "false" })];
        if let Some(t) = &time {
            params.push(("time", t));
        }
        let _: Envelope<serde_json::Value> = self.get("scrobble", &params).await?;
        Ok(())
    }

    /// Songtexte über die OpenSubsonic-Erweiterung `getLyricsBySongId`.
    /// Server ohne diese Erweiterung liefern einen Fehler; den behandeln wir
    /// als "kein Text vorhanden", weil das für den Nutzer dasselbe ist.
    pub async fn lyrics(&self, song_id: &str) -> Result<Option<Lyrics>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "lyricsList", default)]
            list: List,
        }
        #[derive(Deserialize, Default)]
        struct List {
            #[serde(rename = "structuredLyrics", default)]
            structured: Vec<Structured>,
        }
        #[derive(Deserialize)]
        struct Structured {
            #[serde(default)]
            synced: bool,
            #[serde(default)]
            line: Vec<Line>,
        }
        #[derive(Deserialize)]
        struct Line {
            start: Option<u32>,
            #[serde(default)]
            value: String,
        }
        let Ok(r) = self.get::<R>("getLyricsBySongId", &[("id", song_id)]).await else {
            return Ok(None);
        };
        let mut all = r.body.list.structured;
        // Synchronisierte Fassung bevorzugen, falls es mehrere gibt.
        all.sort_by_key(|s| !s.synced);
        Ok(all.into_iter().next().filter(|s| !s.line.is_empty()).map(|s| Lyrics {
            synced: s.synced,
            lines: s
                .line
                .into_iter()
                .map(|l| LyricLine { start_ms: l.start, text: l.value })
                .collect(),
        }))
    }

    /// Alle Titel der Bibliothek, seitenweise (für die lokale Kopie, auf
    /// der die Smart-Playlists rechnen).
    ///
    /// Subsonic hat dafür keinen eigenen Endpunkt. Üblich ist `search3` mit
    /// leerer Suche: Navidrome und andere OpenSubsonic-Server liefern dann
    /// alle Titel.
    pub async fn songs_page(&self, offset: u32, size: u32) -> Result<Vec<Track>> {
        #[derive(Deserialize)]
        struct R {
            #[serde(rename = "searchResult3", default)]
            result: S,
        }
        #[derive(Deserialize, Default)]
        struct S {
            #[serde(default)]
            song: Vec<SSong>,
        }
        let (offset, size) = (offset.to_string(), size.to_string());
        let r: Envelope<R> = self
            .get(
                "search3",
                &[
                    ("query", ""),
                    ("artistCount", "0"),
                    ("albumCount", "0"),
                    ("songCount", &size),
                    ("songOffset", &offset),
                ],
            )
            .await?;
        Ok(r.body.result.song.into_iter().map(Into::into).collect())
    }

    /// Schreibt eine Playlist komplett neu (für gespiegelte Smart-Playlists).
    ///
    /// Mit `existing_id` wird der Inhalt dieser Playlist ersetzt, sonst eine
    /// neue angelegt. Gibt die ID der Playlist zurück, falls der Server sie
    /// meldet.
    ///
    /// POST statt GET: Bei hunderten Titeln wäre die Adresse mit allen IDs
    /// länger, als viele Server und Proxys erlauben.
    pub async fn write_playlist(
        &self,
        existing_id: Option<&str>,
        name: &str,
        song_ids: &[String],
    ) -> Result<Option<String>> {
        #[derive(Deserialize)]
        struct R {
            playlist: Option<SPlaylist>,
        }
        let send = |id: Option<&str>| {
            let mut form: Vec<(&str, &str)> = vec![
                ("u", &self.creds.username),
                ("t", &self.creds.token),
                ("s", &self.creds.salt),
                ("v", API_VERSION),
                ("c", CLIENT_NAME),
                ("f", "json"),
            ];
            match id {
                Some(id) => form.push(("playlistId", id)),
                None => form.push(("name", name)),
            }
            form.extend(song_ids.iter().map(|s| ("songId", s.as_str())));
            let request = self
                .http
                .post(format!("{}/rest/createPlaylist", self.creds.base_url))
                .form(&form);
            async move {
                let body = request.send().await?.error_for_status()?.bytes().await?;
                let env: Envelope<R> = parse_envelope(&body)?;
                Ok::<_, anyhow::Error>(env.body.playlist.map(|p| p.id))
            }
        };
        match existing_id {
            // Wurde die Playlist am Server gelöscht, schlägt das Ersetzen
            // fehl. Dann legen wir sie einfach neu an.
            Some(id) => match send(Some(id)).await {
                Ok(new_id) => Ok(new_id.or_else(|| Some(id.to_owned()))),
                Err(_) => send(None).await,
            },
            None => send(None).await,
        }
    }

    /// Cover-URL in passender Größe.
    ///
    /// AKKU/DATEN: Immer die tatsächlich angezeigte Größe anfordern. Ein
    /// 3000x3000-Cover für ein 56-px-Listenbild kostet Datenvolumen, RAM und
    /// Rechenzeit fürs Verkleinern auf dem Handy.
    pub fn cover_url(&self, cover_id: &str, size: u32) -> String {
        let size = size.to_string();
        self.url("getCoverArt", &[("id", cover_id), ("size", &size)])
    }

    /// Stream-URL in gewünschter Qualität. Der Server wandelt um.
    pub fn stream_url(&self, song_id: &str, q: StreamQuality) -> String {
        let bitrate = q.bitrate_kbps.to_string();
        match q.format {
            // "raw" = Datei unverändert, ohne Umwandlung.
            AudioFormat::Original => self.url("stream", &[("id", song_id), ("format", "raw")]),
            other => {
                let fmt = match other {
                    AudioFormat::Opus => "opus",
                    AudioFormat::Aac => "aac",
                    AudioFormat::Mp3 => "mp3",
                    AudioFormat::Original => unreachable!(),
                };
                self.url(
                    "stream",
                    &[("id", song_id), ("format", fmt), ("maxBitRate", &bitrate)],
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// JSON-Strukturen der Subsonic-API (nur intern; nach außen gehen api::models)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Outer<T> {
    #[serde(rename = "subsonic-response")]
    inner: Envelope<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<T> {
    status: String,
    version: String,
    #[serde(rename = "type")]
    server_type: Option<String>,
    server_version: Option<String>,
    #[serde(default)]
    open_subsonic: bool,
    error: Option<SError>,
    /// Alle übrigen Felder (z. B. `albumList2`) landen im eigentlichen Inhalt.
    #[serde(flatten)]
    body: T,
}

#[derive(Deserialize)]
struct SError {
    code: i32,
    message: Option<String>,
}

fn parse_envelope<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<Envelope<T>> {
    // Erst nur Status/Fehler prüfen: Bei einem Fehler fehlt der Inhalt, und
    // ein direktes Parsen von `T` würde eine unverständliche Meldung liefern.
    let status: Outer<serde_json::Value> =
        serde_json::from_slice(body).context("Antwort ist keine Subsonic-Antwort")?;
    if status.inner.status != "ok" {
        let (code, msg) = status
            .inner
            .error
            .map(|e| (e.code, e.message.unwrap_or_default()))
            .unwrap_or((0, String::new()));
        // Code 40 = falscher Benutzer/Passwort, 41 = Token-Auth nicht unterstützt.
        match code {
            40 => bail!("Benutzername oder Passwort falsch"),
            41 => bail!("Server unterstützt keine Token-Anmeldung (Legacy-Modus nötig)"),
            _ => bail!("Serverfehler {code}: {msg}"),
        }
    }
    let outer: Outer<T> = serde_json::from_slice(body)?;
    Ok(outer.inner)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SAlbum {
    id: String,
    #[serde(alias = "title")]
    name: String,
    artist: Option<String>,
    artist_id: Option<String>,
    cover_art: Option<String>,
    year: Option<u32>,
    song_count: Option<u32>,
    duration: Option<u32>,
    starred: Option<String>,
}

impl From<SAlbum> for Album {
    fn from(a: SAlbum) -> Self {
        Album {
            id: a.id,
            name: a.name,
            artist: a.artist,
            artist_id: a.artist_id,
            cover_art_id: a.cover_art,
            year: a.year,
            song_count: a.song_count,
            duration_secs: a.duration,
            starred: a.starred.is_some(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SPlaylist {
    id: String,
    name: String,
    song_count: Option<u32>,
    duration: Option<u32>,
    cover_art: Option<String>,
    owner: Option<String>,
}

impl From<SPlaylist> for Playlist {
    fn from(p: SPlaylist) -> Self {
        Playlist {
            id: p.id,
            name: p.name,
            song_count: p.song_count,
            duration_secs: p.duration,
            cover_art_id: p.cover_art,
            owner: p.owner,
        }
    }
}

#[derive(Deserialize)]
struct SAlbumWithSongs {
    #[serde(flatten)]
    album: SAlbum,
    #[serde(default)]
    song: Vec<SSong>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SSong {
    id: String,
    title: String,
    artist: Option<String>,
    artist_id: Option<String>,
    album: Option<String>,
    album_id: Option<String>,
    cover_art: Option<String>,
    duration: Option<u32>,
    track: Option<u32>,
    disc_number: Option<u32>,
    year: Option<u32>,
    genre: Option<String>,
    /// Subsonic liefert hier das Datum des Favorisierens – vorhanden = Favorit.
    starred: Option<String>,
    /// OpenSubsonic-Erweiterung (Navidrome liefert das).
    replay_gain: Option<SReplayGain>,
    user_rating: Option<u32>,
    play_count: Option<u32>,
    /// Zeitpunkt des Hinzufügens, z. B. "2024-01-01T12:00:00Z".
    created: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SReplayGain {
    track_gain: Option<f32>,
    album_gain: Option<f32>,
    track_peak: Option<f32>,
    album_peak: Option<f32>,
}

impl From<SSong> for Track {
    fn from(s: SSong) -> Self {
        Track {
            id: s.id,
            title: s.title,
            artist: s.artist,
            artist_id: s.artist_id,
            album: s.album,
            album_id: s.album_id,
            cover_art_id: s.cover_art,
            duration_secs: s.duration,
            track_number: s.track,
            disc_number: s.disc_number,
            year: s.year,
            genre: s.genre,
            starred: s.starred.is_some(),
            replay_gain: s.replay_gain.map(|r| ReplayGainInfo {
                track_gain_db: r.track_gain,
                album_gain_db: r.album_gain,
                track_peak: r.track_peak,
                album_peak: r.album_peak,
            }),
            user_rating: s.user_rating,
            play_count: s.play_count,
            added_at: s.created.as_deref().and_then(parse_iso_utc),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SArtist {
    id: String,
    name: String,
    cover_art: Option<String>,
    album_count: Option<u32>,
    starred: Option<String>,
}

impl From<SArtist> for Artist {
    fn from(a: SArtist) -> Self {
        Artist {
            id: a.id,
            name: a.name,
            cover_art_id: a.cover_art,
            album_count: a.album_count,
            starred: a.starred.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test gegen einen ECHTEN Server: den öffentlichen Navidrome-Demo-Server
    /// (Zugang "demo"/"demo", von Navidrome selbst veröffentlicht).
    ///
    /// Prüft, was die anderen Tests nicht können: ob unsere Strukturen zu den
    /// Antworten eines wirklichen Servers passen. Er braucht Internet und ist
    /// deshalb mit `#[ignore]` markiert; er läuft nur auf ausdrücklichen Wunsch:
    ///
    ///     cargo test live_demo_server -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_demo_server() {
        let client = SubsonicClient::new(SubsonicCredentials::from_password(
            "https://demo.navidrome.org",
            "demo",
            "demo",
        ))
        .unwrap();
        crate::core::runtime::block_on(async {
            let info = client.ping().await.expect("ping");
            println!("Server: {:?} {:?}, OpenSubsonic: {}", info.server_type, info.server_version, info.open_subsonic);

            let newest = client.album_list("newest", 5).await.expect("newest");
            assert!(!newest.is_empty(), "keine Alben");
            println!("Neu: {} Alben, erstes: {}", newest.len(), newest[0].name);

            let recent = client.album_list("recent", 5).await.expect("recent");
            println!("Zuletzt gehoert: {} Alben", recent.len());

            let album = client.album(&newest[0].id).await.expect("album");
            assert!(!album.tracks.is_empty(), "Album ohne Titel");
            let t = &album.tracks[0];
            println!("Titel: {} / {:?} / {:?} s / RG {:?}", t.title, t.artist, t.duration_secs, t.replay_gain);

            let page = client.albums_page(0, 3).await.expect("albums_page");
            assert_eq!(page.len(), 3);

            let artists = client.artists().await.expect("artists");
            assert!(!artists.is_empty());
            let artist = client.artist(&artists[0].id).await.expect("artist");
            println!("Kuenstler: {} ({} Alben)", artist.artist.name, artist.albums.len());

            let search = client.search("love").await.expect("search");
            println!("Suche 'love': {} Titel, {} Alben, {} Kuenstler", search.tracks.len(), search.albums.len(), search.artists.len());

            let songs = client.songs_page(0, 5).await.expect("songs_page");
            assert_eq!(songs.len(), 5, "leere Suche liefert nicht alle Titel");
            println!("Titel-Liste: added_at {:?}, play_count {:?}", songs[0].added_at, songs[0].play_count);

            let playlists = client.playlists().await.expect("playlists");
            println!("Playlists: {}", playlists.len());
            if let Some(p) = playlists.first() {
                let detail = client.playlist(&p.id).await.expect("playlist");
                println!("Playlist '{}': {} Titel", detail.playlist.name, detail.tracks.len());
            }

            let starred = client.starred().await.expect("starred");
            println!("Favoriten: {} Titel, {} Alben, {} Kuenstler", starred.tracks.len(), starred.albums.len(), starred.artists.len());

            let lyrics = client.lyrics(&t.id).await.expect("lyrics");
            println!("Songtext vorhanden: {}", lyrics.is_some());

            // Stream wirklich anfordern: Liefert der Server Audio in der
            // gewünschten Qualität? (Nur die ersten Bytes, kein ganzer Titel.)
            let url = client.stream_url(&t.id, StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 96 });
            let resp = reqwest::Client::new().get(url).send().await.expect("stream");
            println!("Stream: HTTP {} / {:?}", resp.status(), resp.headers().get("content-type"));
            assert!(resp.status().is_success());

            let cover = client.cover_url(t.cover_art_id.as_deref().unwrap_or(""), 64);
            let resp = reqwest::Client::new().get(cover).send().await.expect("cover");
            println!("Cover: HTTP {} / {:?}", resp.status(), resp.headers().get("content-type"));
        });
    }

    #[test]
    fn token_matches_subsonic_docs() {
        // Beispiel direkt aus der Subsonic-API-Doku.
        assert_eq!(make_token("sesame", "c19b2d"), "26719a1196d2a940705a59634eb18eab");
    }

    #[test]
    fn parses_iso_timestamps() {
        assert_eq!(parse_iso_utc("1970-01-01T00:00:00Z"), Some(0));
        // 2024-03-01 liegt nach einem Schalttag: prüft die Datumsformel.
        assert_eq!(parse_iso_utc("2024-03-01T12:00:00.123Z"), Some(1_709_294_400));
        assert_eq!(parse_iso_utc("kaputt"), None);
    }

    #[test]
    fn base_url_is_normalized() {
        assert_eq!(normalize_base_url("music.local:4533/"), "https://music.local:4533");
        assert_eq!(normalize_base_url("http://x/rest"), "http://x");
    }

    #[test]
    fn parses_album_with_songs() {
        let json = br#"{"subsonic-response":{"status":"ok","version":"1.16.1",
            "type":"navidrome","serverVersion":"0.58","openSubsonic":true,
            "album":{"id":"al1","name":"Album","artist":"K","coverArt":"al-1","songCount":1,
              "song":[{"id":"s1","title":"Lied","track":1,"duration":200,
                "starred":"2024-01-01T00:00:00Z","replayGain":{"trackGain":-6.5,"trackPeak":0.98}}]}}}"#;
        #[derive(Deserialize)]
        struct R {
            album: SAlbumWithSongs,
        }
        let env: Envelope<R> = parse_envelope(json).unwrap();
        assert!(env.open_subsonic);
        let t: Track = env.body.album.song.into_iter().next().unwrap().into();
        assert!(t.starred);
        assert_eq!(t.replay_gain.unwrap().track_gain_db, Some(-6.5));
    }

    #[test]
    fn wrong_password_gives_clear_error() {
        let json = br#"{"subsonic-response":{"status":"failed","version":"1.16.1",
            "error":{"code":40,"message":"Wrong username or password"}}}"#;
        let err = parse_envelope::<serde_json::Value>(json).err().unwrap();
        assert!(err.to_string().contains("Passwort falsch"));
    }

    #[test]
    fn stream_url_requests_transcoding() {
        let c = SubsonicClient::new(SubsonicCredentials {
            base_url: "https://m.example".into(),
            username: "u".into(),
            token: "t".into(),
            salt: "s".into(),
        })
        .unwrap();
        let u = c.stream_url("42", StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 128 });
        assert!(u.starts_with("https://m.example/rest/stream?"));
        assert!(u.contains("format=opus") && u.contains("maxBitRate=128") && u.contains("id=42"));
    }
}
