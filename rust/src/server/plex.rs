//! Adapter für Plex Media Server.
//!
//! Doku (inoffiziell, vom Hersteller kaum dokumentiert): die Plex-Entwickler-
//! Seiten <https://developer.plex.tv/pms/> und das Python-Projekt `python-plexapi`,
//! an dem sich die Parameter hier orientieren.
//!
//! # Anmeldung
//! Plex meldet nicht am eigenen Server an, sondern bei `plex.tv`:
//! `POST https://plex.tv/api/v2/users/signin` mit Benutzername und Passwort
//! liefert ein `authToken`. Mit diesem Token spricht man dann den Server an
//! (Parameter `X-Plex-Token`). Gespeichert wird NUR das Token, nie das
//! Passwort. Alternativ kann man ein fertiges Token eintragen: Bleibt der
//! Benutzername leer, gilt das Passwort-Feld als Token (nützlich bei Konten
//! mit Zwei-Faktor-Anmeldung oder Single-Sign-On, wo es kein Passwort gibt).
//!
//! # Musikbibliotheken
//! Ein Plex-Server hat mehrere "Sections" (Filme, Serien, Musik …). Beim
//! Anmelden suchen wir alle vom Typ `artist` (so heißt Musik bei Plex) und
//! merken uns ihre IDs. Alle Listen laufen dann über diese Sections. Kommt
//! später eine Musikbibliothek dazu, ist eine neue Anmeldung nötig.
//!
//! # Was bei Plex anders ist (und wie wir es abbilden)
//! * **IDs**: Jedes Ding hat einen `ratingKey` (Zahl als Text). Das ist unsere ID.
//! * **Favoriten**: Plex hat keine Favoriten, nur Bewertungen. Wie in Plexamp
//!   gilt "Herz" als Bewertung 10 (5 Sterne); ab 9 zählt ein Eintrag bei uns
//!   als Favorit. Entfernen löscht die Bewertung.
//! * **Datei-Pfad für den Stream**: Die Original-Datei liegt unter einem
//!   Pfad wie `/library/parts/123/1700000000/file.flac`, den man nur aus den
//!   Metadaten des Titels erfährt. `stream_url` ist aber synchron und ohne
//!   Netz. Wir merken uns deshalb jeden Pfad, den wir beim Laden von Titeln
//!   sehen (siehe `parts`), und fragen den Server erst bei einem Titel an,
//!   den wir noch nie gesehen haben (z. B. aus der gespeicherten Warteschlange
//!   nach einem App-Neustart): Dort fällt die URL auf den Transcoder zurück.
//! * **Songtexte**: Kommen als eigener "Stream" am Titel (Plex Pass nötig).
//! * **Wiedergabe-Datum**: `scrobble` setzt immer "jetzt"; Plex nimmt kein
//!   eigenes Datum an. Offline Gehörtes bekommt daher das Datum der Übertragung.
//!
//! # Was NICHT gegen einen echten Server geprüft ist
//! Es gibt keinen öffentlichen Plex-Testserver. Dieser Adapter ist nur durch
//! Unit-Tests mit Beispiel-JSON abgesichert; Pfade und Parameter folgen der
//! Doku bzw. `python-plexapi`. Besonders unsicher: Umwandeln nach Opus/AAC,
//! Songtexte, Bewertung entfernen.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use rand::distributions::{Alphanumeric, DistString};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::api::models::{
    Album, AlbumDetail, Artist, ArtistDetail, LyricLine, Lyrics, Playlist, PlaylistDetail,
    SearchResult, ServerInfo, ServerKind, StarKind, Starred, Track,
};
use crate::api::settings::{AudioFormat, StreamQuality};

const CLIENT_NAME: &str = "Nocturne";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Anmelde-Server von Plex. Fest eingebaut, nicht vom Nutzer wählbar: An
/// diese Adresse geht das Passwort, da darf nichts umgeleitet werden.
const SIGNIN_URL: &str = "https://plex.tv/api/v2/users/signin";
/// Kennung der Bibliotheks-Schnittstelle, die Plex bei Bewertungen u. ä. verlangt.
const IDENTIFIER: &str = "com.plexapp.plugins.library";

/// Plex-Typnummern für `?type=` bei Abfragen auf einer Section.
const TYPE_ARTIST: u32 = 8;
const TYPE_ALBUM: u32 = 9;
const TYPE_TRACK: u32 = 10;

/// Ab dieser Bewertung (0–10) gilt etwas als Favorit.
const FAVORITE_MIN_RATING: f64 = 9.0;
/// So viele Titel pro Anfrage beim Schreiben von Playlists (Länge der Adresse).
const PLAYLIST_CHUNK: usize = 100;

/// Was gespeichert wird. Enthält KEIN Passwort, nur das Token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlexCredentials {
    /// Adresse des Servers, z. B. `http://192.168.1.5:32400`
    pub base_url: String,
    pub token: String,
    /// Zufällige Kennung dieser App-Installation (Plex verlangt sie).
    pub client_id: String,
    /// IDs der Musikbibliotheken, beim Anmelden gefunden.
    #[serde(default)]
    pub sections: Vec<String>,
    /// Kennung des Servers; steckt in Playlist-Adressen. Leer = noch nicht
    /// bekannt, wird bei Bedarf nachgeladen.
    #[serde(default)]
    pub machine_id: String,
}

/// Wie `subsonic::normalize_base_url`, nur für Plex-Adressen.
fn normalize_base_url(input: &str) -> String {
    let mut s = input.trim().trim_end_matches('/').to_owned();
    if !s.starts_with("http://") && !s.starts_with("https://") {
        // HTTPS als Standard: Bei HTTP gehen Token und Musik im Klartext übers Netz.
        s = format!("https://{s}");
    }
    // Aus der Adresszeile des Webplayers kopiert: "…/web/index.html#!/…".
    if let Some(i) = s.find('#') {
        s.truncate(i);
    }
    for suffix in ["/web/index.html", "/web"] {
        if let Some(stripped) = s.strip_suffix(suffix) {
            s = stripped.to_owned();
        }
    }
    s
}

pub fn new_client_id() -> String {
    format!("nocturne-{}", Alphanumeric.sample_string(&mut rand::thread_rng(), 16))
}

/// Kopfzeilen, mit denen sich ein Plex-Client vorstellt.
fn identity_headers(client_id: &str) -> Result<HeaderMap> {
    let mut h = HeaderMap::new();
    let mut put = |name: &'static str, value: &str| -> Result<()> {
        h.insert(HeaderName::from_static(name), HeaderValue::from_str(value)?);
        Ok(())
    };
    // Ohne diese Zeile antwortet Plex mit XML statt JSON.
    put("accept", "application/json")?;
    put("x-plex-client-identifier", client_id)?;
    put("x-plex-product", CLIENT_NAME)?;
    put("x-plex-version", CLIENT_VERSION)?;
    put("x-plex-platform", "Android")?;
    put("x-plex-device-name", CLIENT_NAME)?;
    Ok(h)
}

fn build_http(client_id: &str) -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // Lieber früh einen klaren Fehler anzeigen, als ewig zu hängen,
        // wenn der Server zu Hause nicht erreichbar ist.
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .default_headers(identity_headers(client_id)?)
        .build()?)
}

/// Meldet bei plex.tv an und gibt das Token zurück. Das Passwort wird nur
/// in diesem einen Aufruf verwendet.
pub async fn sign_in(username: &str, password: &str, client_id: &str) -> Result<String> {
    let resp = build_http(client_id)?
        .post(SIGNIN_URL)
        .form(&[("login", username), ("password", password), ("rememberMe", "true")])
        .send()
        .await
        .context("plex.tv nicht erreichbar")?;
    let status = resp.status();
    parse_signin(status, &resp.bytes().await?)
}

/// Wertet die Antwort von plex.tv aus. Eigene Funktion, damit sich die
/// Fehlermeldungen ohne Netzwerk testen lassen.
fn parse_signin(status: StatusCode, body: &[u8]) -> Result<String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Ok_ {
        auth_token: String,
    }
    #[derive(Deserialize, Default)]
    struct Errors {
        #[serde(default)]
        errors: Vec<Err_>,
    }
    #[derive(Deserialize)]
    struct Err_ {
        code: Option<i64>,
        message: Option<String>,
    }
    if status.is_success() {
        return serde_json::from_slice::<Ok_>(body)
            .map(|r| r.auth_token)
            .context("Antwort stammt nicht von plex.tv");
    }
    let first = serde_json::from_slice::<Errors>(body).unwrap_or_default().errors.into_iter().next();
    let code = first.as_ref().and_then(|e| e.code);
    let message = first.and_then(|e| e.message).unwrap_or_default();
    if code == Some(1029) || message.to_lowercase().contains("verification code") {
        // Plex erlaubt, den Code einfach hinten ans Passwort zu hängen.
        bail!("Zwei-Faktor-Anmeldung aktiv: Code direkt hinter das Passwort schreiben (ohne Leerzeichen)");
    }
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!("Benutzername oder Passwort falsch"),
        StatusCode::TOO_MANY_REQUESTS => bail!("Zu viele Anmeldeversuche bei Plex – bitte später erneut versuchen"),
        s => bail!("Plex-Anmeldung fehlgeschlagen (HTTP {}) {message}", s.as_u16()),
    }
}

/// Übersetzt einen Fehlerstatus des Servers in eine verständliche Meldung.
fn status_error(status: StatusCode) -> Option<anyhow::Error> {
    if status.is_success() {
        None
    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        // Das Token wurde widerrufen oder war falsch (bei Token-Eingabe).
        Some(anyhow!("Plex-Token ungültig oder abgelaufen – bitte neu anmelden"))
    } else {
        Some(anyhow!("Serverfehler (HTTP {})", status.as_u16()))
    }
}

pub struct PlexClient {
    creds: PlexCredentials,
    http: reqwest::Client,
    /// ratingKey eines Titels -> Pfad seiner Datei (`/library/parts/…`).
    /// Nur im Speicher: Er füllt sich beim Durchblättern der Bibliothek.
    parts: RwLock<HashMap<String, String>>,
}

impl PlexClient {
    pub fn new(creds: PlexCredentials) -> Result<Self> {
        let http = build_http(&creds.client_id)?;
        Ok(Self { creds, http, parts: RwLock::new(HashMap::new()) })
    }

    pub fn credentials(&self) -> &PlexCredentials {
        &self.creds
    }

    /// Verbindet mit dem Server, prüft das Token und sucht die Musikbibliotheken.
    pub async fn connect(base_url: &str, token: &str, client_id: &str) -> Result<Self> {
        let mut client = Self::new(PlexCredentials {
            base_url: normalize_base_url(base_url),
            token: token.to_owned(),
            client_id: client_id.to_owned(),
            sections: Vec::new(),
            machine_id: String::new(),
        })?;
        // `/` verlangt ein gültiges Token: Ein falsches fällt hier auf.
        let root = client.container(&[], &[]).await?;
        let sections: Container = client.container(&["library", "sections"], &[]).await?;
        client.creds.machine_id = root.machine_identifier.unwrap_or_default();
        client.creds.sections = music_sections(&sections);
        if client.creds.sections.is_empty() {
            bail!("Auf diesem Plex-Server wurde keine Musikbibliothek gefunden");
        }
        Ok(client)
    }

    // -----------------------------------------------------------------------
    // HTTP-Grundlagen
    // -----------------------------------------------------------------------

    /// Baut eine vollständige URL inkl. Token. Pfadteile werden einzeln
    /// übergeben und dabei prozentkodiert (siehe Jellyfin-Adapter).
    fn url(&self, segments: &[&str], params: &[(&str, &str)]) -> String {
        let mut url = Url::parse(&self.creds.base_url)
            .unwrap_or_else(|_| Url::parse("https://invalid.invalid").unwrap());
        if let Ok(mut p) = url.path_segments_mut() {
            p.pop_if_empty();
            p.extend(segments);
        }
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("X-Plex-Token", &self.creds.token);
            for (k, v) in params {
                q.append_pair(k, v);
            }
        }
        url.into()
    }

    async fn send(&self, method: Method, segments: &[&str], params: &[(&str, &str)]) -> Result<reqwest::Response> {
        let resp = self
            .http
            .request(method, self.url(segments, params))
            .send()
            .await
            .with_context(|| format!("Server nicht erreichbar ({})", segments.join("/")))?;
        match status_error(resp.status()) {
            Some(e) => Err(e),
            None => Ok(resp),
        }
    }

    /// Fragt ab und packt die `MediaContainer`-Hülle aus. Merkt sich dabei
    /// die Datei-Pfade aller enthaltenen Titel.
    async fn container(&self, segments: &[&str], params: &[(&str, &str)]) -> Result<Container> {
        let bytes = self.send(Method::GET, segments, params).await?.bytes().await?;
        let root: Root = serde_json::from_slice(&bytes).context("Unerwartete Antwort vom Server")?;
        self.remember_parts(&root.container.metadata);
        for hub in &root.container.hub {
            self.remember_parts(&hub.metadata);
        }
        Ok(root.container)
    }

    fn remember_parts(&self, items: &[PItem]) {
        let found: Vec<(&str, &str)> = items
            .iter()
            .filter_map(|i| i.part_key().map(|k| (i.rating_key.as_str(), k)))
            .collect();
        if found.is_empty() {
            return;
        }
        let mut map = self.parts.write().unwrap();
        for (id, key) in found {
            map.insert(id.to_owned(), key.to_owned());
        }
    }

    /// `type=…`-Abfrage auf einer Section.
    async fn section_items(
        &self,
        section: &str,
        item_type: u32,
        sort: &str,
        start: u32,
        size: Option<u32>,
        extra: &[(&str, &str)],
    ) -> Result<Container> {
        let (item_type, start) = (item_type.to_string(), start.to_string());
        let size = size.map(|s| s.to_string());
        let mut params: Vec<(&str, &str)> = vec![("type", &item_type), ("sort", sort)];
        // Ohne Größenangabe liefert Plex alles auf einmal.
        if let Some(size) = &size {
            params.push(("X-Plex-Container-Start", &start));
            params.push(("X-Plex-Container-Size", size));
        }
        params.extend_from_slice(extra);
        self.container(&["library", "sections", section, "all"], &params).await
    }

    fn sections(&self) -> Result<&[String]> {
        if self.creds.sections.is_empty() {
            bail!("Keine Musikbibliothek bekannt – bitte neu anmelden");
        }
        Ok(&self.creds.sections)
    }

    /// Seitenweise über ALLE Musikbibliotheken wie über eine einzige Liste.
    /// Meist gibt es nur eine; dann ist das eine einzige Anfrage.
    async fn paged(
        &self,
        item_type: u32,
        sort: &str,
        offset: u32,
        size: u32,
        extra: &[(&str, &str)],
    ) -> Result<Vec<PItem>> {
        let (mut skip, mut remaining) = (offset, size);
        let mut out = Vec::new();
        for section in self.sections()? {
            if remaining == 0 {
                break;
            }
            let c = self.section_items(section, item_type, sort, skip, Some(remaining), extra).await?;
            let total = c.total_size.unwrap_or(skip);
            let got = c.metadata.len() as u32;
            (skip, remaining) = advance_paging(skip, remaining, got, total);
            out.extend(c.metadata);
        }
        Ok(out)
    }

    /// Alles einer Art aus allen Musikbibliotheken (ohne Seiten).
    async fn all(&self, item_type: u32, sort: &str, extra: &[(&str, &str)]) -> Result<Vec<PItem>> {
        let mut out = Vec::new();
        for section in self.sections()? {
            out.extend(self.section_items(section, item_type, sort, 0, None, extra).await?.metadata);
        }
        Ok(out)
    }

    /// Ein Eintrag samt Kindern (Künstler -> Alben, Album -> Titel).
    async fn item_and_children(&self, id: &str) -> Result<(PItem, Vec<PItem>)> {
        let item = self
            .container(&["library", "metadata", id], &[])
            .await?
            .metadata
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Eintrag nicht gefunden"))?;
        let children = self.container(&["library", "metadata", id, "children"], &[]).await?.metadata;
        Ok((item, children))
    }

    // -----------------------------------------------------------------------
    // Schnittstelle von ServerClient
    // -----------------------------------------------------------------------

    pub async fn ping(&self) -> Result<ServerInfo> {
        let c = self.container(&[], &[]).await?;
        Ok(ServerInfo {
            kind: ServerKind::Plex,
            server_type: Some("Plex Media Server".to_owned()),
            server_version: c.version.clone(),
            api_version: c.version.unwrap_or_default(),
            // Gibt es nur bei Subsonic-Servern.
            open_subsonic: false,
        })
    }

    /// Zuletzt hinzugefügte Alben.
    pub async fn newest_albums(&self, count: u32) -> Result<Vec<Album>> {
        // Pro Bibliothek `count` holen und danach mischen: So stimmt die
        // Reihenfolge auch bei mehreren Musikbibliotheken.
        let mut items = self.paged_each_section(TYPE_ALBUM, "addedAt:desc", count).await?;
        items.sort_by_key(|i| std::cmp::Reverse(i.added_at.unwrap_or(0)));
        items.truncate(count as usize);
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Zuletzt gehörte Alben. Plex sortiert Nie-Gehörtes ans Ende; das
    /// filtern wir heraus.
    pub async fn recent_albums(&self, count: u32) -> Result<Vec<Album>> {
        let mut items = self.paged_each_section(TYPE_ALBUM, "lastViewedAt:desc", count).await?;
        items.retain(|i| i.last_viewed_at.is_some());
        items.sort_by_key(|i| std::cmp::Reverse(i.last_viewed_at.unwrap_or(0)));
        items.truncate(count as usize);
        Ok(items.into_iter().map(Into::into).collect())
    }

    async fn paged_each_section(&self, item_type: u32, sort: &str, count: u32) -> Result<Vec<PItem>> {
        let mut out = Vec::new();
        for section in self.sections()? {
            out.extend(self.section_items(section, item_type, sort, 0, Some(count), &[]).await?.metadata);
        }
        Ok(out)
    }

    /// Alle Alben alphabetisch, seitenweise.
    pub async fn albums_page(&self, offset: u32, size: u32) -> Result<Vec<Album>> {
        let items = self.paged(TYPE_ALBUM, "titleSort", offset, size, &[]).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn album(&self, id: &str) -> Result<AlbumDetail> {
        let (album, tracks) = self.item_and_children(id).await?;
        // Titel haben selten eigene Genres; dann gilt das des Albums.
        let genre = album.genre.first().map(|g| g.tag.clone());
        Ok(AlbumDetail {
            album: album.into(),
            tracks: tracks
                .into_iter()
                .map(|t| {
                    let mut t: Track = t.into();
                    t.genre = t.genre.or_else(|| genre.clone());
                    t
                })
                .collect(),
        })
    }

    pub async fn artists(&self) -> Result<Vec<Artist>> {
        let items = self.all(TYPE_ARTIST, "titleSort", &[]).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        let (artist, albums) = self.item_and_children(id).await?;
        let mut artist: Artist = artist.into();
        artist.album_count = artist.album_count.or(Some(albums.len() as u32));
        Ok(ArtistDetail { artist, albums: albums.into_iter().map(Into::into).collect() })
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let c = self.container(&["playlists"], &[("playlistType", "audio")]).await?;
        Ok(c.metadata.into_iter().map(Into::into).collect())
    }

    pub async fn playlist(&self, id: &str) -> Result<PlaylistDetail> {
        let playlist = self
            .container(&["playlists", id], &[])
            .await?
            .metadata
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Playlist nicht gefunden"))?;
        let tracks = self.container(&["playlists", id, "items"], &[]).await?.metadata;
        Ok(PlaylistDetail {
            playlist: playlist.into(),
            tracks: tracks.into_iter().map(Into::into).collect(),
        })
    }

    /// Adresse, mit der Plex Titel für Playlists anspricht:
    /// `server://<Server-ID>/com.plexapp.plugins.library/library/metadata/<id,id,…>`
    async fn playlist_uri(&self, song_ids: &[String]) -> Result<String> {
        let machine_id = if self.creds.machine_id.is_empty() {
            self.container(&[], &[]).await?.machine_identifier.unwrap_or_default()
        } else {
            self.creds.machine_id.clone()
        };
        if machine_id.is_empty() {
            bail!("Server hat seine Kennung nicht mitgeteilt");
        }
        Ok(format!("server://{machine_id}/{IDENTIFIER}/library/metadata/{}", song_ids.join(",")))
    }

    async fn create_playlist_id(&self, name: &str, song_ids: &[String]) -> Result<String> {
        // Plex legt Playlists nur MIT mindestens einem Titel an.
        let first = &song_ids[..song_ids.len().min(PLAYLIST_CHUNK)];
        if first.is_empty() {
            bail!("Plex kann keine leere Playlist anlegen");
        }
        let uri = self.playlist_uri(first).await?;
        let bytes = self
            .send(
                Method::POST,
                &["playlists"],
                &[("type", "audio"), ("title", name), ("smart", "0"), ("uri", &uri)],
            )
            .await?
            .bytes()
            .await?;
        let root: Root = serde_json::from_slice(&bytes).context("Unerwartete Antwort vom Server")?;
        let id = root
            .container
            .metadata
            .into_iter()
            .next()
            .map(|p| p.rating_key)
            .ok_or_else(|| anyhow!("Server hat die Playlist-ID nicht mitgeteilt"))?;
        self.append_to_playlist(&id, &song_ids[first.len()..]).await?;
        Ok(id)
    }

    async fn append_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        for chunk in song_ids.chunks(PLAYLIST_CHUNK) {
            let uri = self.playlist_uri(chunk).await?;
            self.send(Method::PUT, &["playlists", playlist_id, "items"], &[("uri", &uri)]).await?;
        }
        Ok(())
    }

    pub async fn create_playlist(&self, name: &str, song_ids: &[String]) -> Result<()> {
        self.create_playlist_id(name, song_ids).await.map(|_| ())
    }

    pub async fn add_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        self.append_to_playlist(playlist_id, song_ids).await
    }

    /// Alle Favoriten (Bewertung ab 9, siehe oben).
    pub async fn starred(&self) -> Result<Starred> {
        // `userRating>>=9` heißt bei Plex "größer oder gleich": Der Operator
        // gehört zum Parameternamen. Zusätzlich filtern wir selbst nach, falls
        // ein Server den Filter ignoriert.
        let filter = [("userRating>>", "9")];
        let keep = |items: Vec<PItem>| -> Vec<PItem> {
            items.into_iter().filter(|i| i.user_rating.is_some_and(|r| r >= FAVORITE_MIN_RATING)).collect()
        };
        let tracks = keep(self.all(TYPE_TRACK, "titleSort", &filter).await?);
        let albums = keep(self.all(TYPE_ALBUM, "titleSort", &filter).await?);
        let artists = keep(self.all(TYPE_ARTIST, "titleSort", &filter).await?);
        Ok(Starred {
            artists: artists.into_iter().map(Into::into).collect(),
            albums: albums.into_iter().map(Into::into).collect(),
            tracks: tracks.into_iter().map(Into::into).collect(),
        })
    }

    /// Favorit setzen (Bewertung 10) oder entfernen (Bewertung löschen, `-1`).
    /// Achtung: Wer in Plex eigene Sternebewertungen vergibt, verliert beim
    /// Entfernen des Herzens auch diese.
    pub async fn set_starred(&self, _kind: StarKind, id: &str, starred: bool) -> Result<()> {
        let rating = if starred { "10" } else { "-1" };
        self.send(
            Method::PUT,
            &[":", "rate"],
            &[("key", id), ("identifier", IDENTIFIER), ("rating", rating)],
        )
        .await?;
        Ok(())
    }

    /// Meldet dem Server eine Wiedergabe.
    ///
    /// `submission = true`: Titel wurde gehört (`/:/scrobble` erhöht den
    /// Zähler und setzt "zuletzt gehört"). Plex nimmt dabei kein eigenes
    /// Datum an, `time_ms` wird deshalb nicht verwendet.
    ///
    /// `submission = false`: "läuft gerade" über `/:/timeline`. Wir melden
    /// nur den Start (kein laufendes Aktualisieren: spart Akku); Plex lässt
    /// die Anzeige nach kurzer Zeit von selbst auslaufen.
    pub async fn scrobble(&self, song_id: &str, _time_ms: Option<i64>, submission: bool) -> Result<()> {
        if submission {
            self.send(Method::GET, &[":", "scrobble"], &[("key", song_id), ("identifier", IDENTIFIER)]).await?;
        } else {
            let key = format!("/library/metadata/{song_id}");
            self.send(
                Method::GET,
                &[":", "timeline"],
                &[
                    ("ratingKey", song_id),
                    ("key", &key),
                    ("identifier", IDENTIFIER),
                    ("state", "playing"),
                    ("time", "0"),
                ],
            )
            .await?;
        }
        Ok(())
    }

    /// Songtext aus dem Songtext-Stream des Titels (Plex Pass). Jeder Fehler
    /// heißt für den Nutzer "kein Text", deshalb `None` statt Fehler.
    pub async fn lyrics(&self, song_id: &str) -> Result<Option<Lyrics>> {
        let Ok(c) = self.container(&["library", "metadata", song_id], &[]).await else {
            return Ok(None);
        };
        let streams: Vec<&PStream> = c
            .metadata
            .iter()
            .flat_map(|m| &m.media)
            .flat_map(|m| &m.part)
            .flat_map(|p| &p.stream)
            .filter(|s| s.stream_type == Some(4) && s.key.is_some())
            .collect();
        // Zeitlich synchronisierte Fassung (LRC) bevorzugen.
        let Some(stream) = streams.iter().find(|s| s.format.as_deref() == Some("lrc")).or(streams.first()) else {
            return Ok(None);
        };
        let Some(key) = &stream.key else { return Ok(None) };
        let Ok(mut url) = Url::parse(&format!("{}{key}", self.creds.base_url)) else {
            return Ok(None);
        };
        url.query_pairs_mut().append_pair("X-Plex-Token", &self.creds.token);
        // Der Songtext kommt als Klartext, nicht als JSON.
        let Ok(resp) = self.http.get(url).header("accept", "*/*").send().await else {
            return Ok(None);
        };
        if !resp.status().is_success() {
            return Ok(None);
        }
        Ok(parse_lrc(&resp.text().await.unwrap_or_default()))
    }

    /// Suche über Titel, Alben und Künstler (`/hubs/search`).
    pub async fn search(&self, query: &str) -> Result<SearchResult> {
        let mut result = SearchResult::default();
        for section in self.sections()? {
            let c = self
                .container(&["hubs", "search"], &[("query", query), ("sectionId", section), ("limit", "50")])
                .await?;
            // Die Antwort ist in "Hubs" je Art gegliedert; die Art steht
            // aber auch an jedem Eintrag, daran sortieren wir.
            for item in c.hub.into_iter().flat_map(|h| h.metadata) {
                match item.kind.as_deref() {
                    Some("artist") => result.artists.push(item.into()),
                    Some("album") => result.albums.push(item.into()),
                    Some("track") => result.tracks.push(item.into()),
                    _ => {}
                }
            }
        }
        result.artists.truncate(10);
        result.albums.truncate(20);
        result.tracks.truncate(50);
        Ok(result)
    }

    /// Alle Titel der Bibliothek, seitenweise (für die lokale Kopie, auf der
    /// die Smart-Playlists rechnen).
    pub async fn songs_page(&self, offset: u32, size: u32) -> Result<Vec<Track>> {
        let items = self.paged(TYPE_TRACK, "titleSort", offset, size, &[]).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Schreibt eine Playlist komplett neu (für gespiegelte Smart-Playlists).
    ///
    /// Mit `existing_id`: alle Einträge löschen, dann die neuen anfügen. Gibt
    /// es die Playlist nicht mehr, wird sie neu angelegt. Ohne Titel und ohne
    /// bestehende Playlist passiert nichts (`None`), weil Plex keine leeren
    /// Playlists anlegt.
    pub async fn write_playlist(
        &self,
        existing_id: Option<&str>,
        name: &str,
        song_ids: &[String],
    ) -> Result<Option<String>> {
        if let Some(id) = existing_id {
            let replaced = async {
                self.send(Method::DELETE, &["playlists", id, "items"], &[]).await?;
                self.append_to_playlist(id, song_ids).await
            };
            if replaced.await.is_ok() {
                return Ok(Some(id.to_owned()));
            }
        }
        if song_ids.is_empty() {
            return Ok(None);
        }
        self.create_playlist_id(name, song_ids).await.map(Some)
    }

    /// Cover-URL in passender Größe, über den Bild-Umwandler des Servers.
    ///
    /// AKKU/DATEN: Immer die tatsächlich angezeigte Größe anfordern; der
    /// Server verkleinert, das Handy muss kein 3000-px-Bild laden.
    ///
    /// `cover_id` ist der `ratingKey` von Album/Künstler; Playlists haben
    /// kein eigenes Bild und tragen den Vorsatz `playlist:` (ihr Bild ist
    /// eine Collage, die der Server erzeugt).
    pub fn cover_url(&self, cover_id: &str, size: u32) -> String {
        let inner = match cover_id.strip_prefix("playlist:") {
            Some(pl) => format!("/playlists/{pl}/composite"),
            None => format!("/library/metadata/{cover_id}/thumb"),
        };
        let size = size.to_string();
        self.url(
            &["photo", ":", "transcode"],
            &[("url", &inner), ("width", &size), ("height", &size), ("minSize", "1"), ("upscale", "1")],
        )
    }

    /// Stream-URL in gewünschter Qualität.
    ///
    /// Original: die Datei selbst (Springen im Titel per Range-Anfrage),
    /// sofern wir ihren Pfad kennen. Sonst, und bei jeder anderen Qualität,
    /// der Transcoder des Servers (`/music/:/transcode/universal`).
    pub fn stream_url(&self, song_id: &str, q: StreamQuality) -> String {
        if q.format == AudioFormat::Original {
            let known = self.parts.read().unwrap().get(song_id).cloned();
            if let Some(key) = known {
                if let Ok(mut url) = Url::parse(&format!("{}{key}", self.creds.base_url)) {
                    url.query_pairs_mut().append_pair("X-Plex-Token", &self.creds.token);
                    return url.into();
                }
            }
        }
        self.transcode_url(song_id, q)
    }

    /// Adresse des Transcoders. Bei `Original` (Pfad unbekannt) darf der Server
    /// den Ton unverändert durchreichen, sonst wandelt er um.
    fn transcode_url(&self, song_id: &str, q: StreamQuality) -> String {
        let path = format!("/library/metadata/{song_id}");
        let bitrate = q.bitrate_kbps.max(32).to_string();
        let (file, container, passthrough) = match q.format {
            AudioFormat::Original => ("start", "mp3", true),
            AudioFormat::Opus => ("start.opus", "opus", false),
            AudioFormat::Aac => ("start.aac", "aac", false),
            AudioFormat::Mp3 => ("start.mp3", "mp3", false),
        };
        // Plex wählt das Zielformat anhand eines "Client-Profils". Wir legen
        // es mit dieser Zusatzzeile selbst fest.
        let profile = format!(
            "add-transcode-target(type=musicProfile&context=streaming&protocol=http&container={container}&audioCodec={container})"
        );
        let pass = if passthrough { "1" } else { "0" };
        let mut params: Vec<(&str, &str)> = vec![
            ("path", &path),
            ("mediaIndex", "0"),
            ("partIndex", "0"),
            ("protocol", "http"),
            ("directPlay", pass),
            ("directStream", pass),
            ("X-Plex-Client-Identifier", &self.creds.client_id),
            ("X-Plex-Product", CLIENT_NAME),
            ("X-Plex-Platform", "Android"),
            ("X-Plex-Client-Profile-Extra", &profile),
        ];
        if !passthrough {
            params.push(("audioBitrate", &bitrate));
            params.push(("maxAudioBitrate", &bitrate));
        }
        self.url(&["music", ":", "transcode", "universal", file], &params)
    }
}

/// Rechnet nach einer Teilabfrage aus, wie es beim seitenweisen Lesen über
/// mehrere Bibliotheken weitergeht. `got` Einträge kamen, die Bibliothek hat
/// `total` insgesamt. Gibt (zu überspringende Einträge, noch fehlende) zurück.
fn advance_paging(skip: u32, remaining: u32, got: u32, total: u32) -> (u32, u32) {
    if got == 0 {
        // Nichts geliefert: Der Anfang der Seite liegt noch hinter dieser
        // Bibliothek, also diese ganz überspringen.
        (skip.saturating_sub(total), remaining)
    } else {
        (0, remaining.saturating_sub(got))
    }
}

/// IDs aller Musikbibliotheken (Typ `artist`) aus `/library/sections`.
fn music_sections(c: &Container) -> Vec<String> {
    c.directory.iter().filter(|d| d.kind.as_deref() == Some("artist")).map(|d| d.key.clone()).collect()
}

/// Liest einen Songtext im LRC-Format (`[01:23.45]Text`). Zeilen ohne
/// Zeitstempel werden zu normalem Text; Kopfzeilen wie `[ar:Name]` fallen weg.
fn parse_lrc(text: &str) -> Option<Lyrics> {
    // "mm:ss.xx" oder "mm:ss" -> Millisekunden.
    fn stamp(s: &str) -> Option<u32> {
        let (min, rest) = s.split_once(':')?;
        let (sec, frac) = rest.split_once('.').unwrap_or((rest, "0"));
        let frac_ms = format!("{frac:0<3}")[..3].parse::<u32>().ok()?;
        Some((min.parse::<u32>().ok()? * 60 + sec.parse::<u32>().ok()?) * 1000 + frac_ms)
    }
    let mut lines = Vec::new();
    let mut synced = false;
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        // Eine Zeile darf mehrere Stempel haben: "[00:10.0][00:50.0]Refrain".
        while let Some(inner) = rest.strip_prefix('[').and_then(|r| r.split_once(']')) {
            match stamp(inner.0) {
                Some(t) => times.push(t),
                None if times.is_empty() => {
                    // Kopfzeile wie [ar:Name]: ganze Zeile ignorieren.
                    rest = "";
                    break;
                }
                None => break,
            }
            rest = inner.1;
        }
        let body = rest.trim();
        if times.is_empty() {
            if !body.is_empty() {
                lines.push(LyricLine { start_ms: None, text: body.to_owned() });
            }
        } else {
            synced = true;
            lines.extend(times.into_iter().map(|t| LyricLine { start_ms: Some(t), text: body.to_owned() }));
        }
    }
    if lines.is_empty() {
        return None;
    }
    if synced {
        // Zeilen ohne Stempel in einem synchronisierten Text würden das
        // Mitlaufen stören; Zeilen nach Zeit ordnen.
        lines.retain(|l| l.start_ms.is_some());
        lines.sort_by_key(|l| l.start_ms);
    }
    Some(Lyrics { synced, lines })
}

// ---------------------------------------------------------------------------
// JSON-Strukturen (nur intern; nach außen gehen api::models)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Root {
    #[serde(rename = "MediaContainer")]
    container: Container,
}

/// Die Hülle jeder Plex-Antwort. Je nach Abfrage ist nur ein Teil gefüllt.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Container {
    total_size: Option<u32>,
    machine_identifier: Option<String>,
    version: Option<String>,
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PItem>,
    #[serde(rename = "Directory", default)]
    directory: Vec<PDirectory>,
    #[serde(rename = "Hub", default)]
    hub: Vec<PHub>,
}

#[derive(Deserialize)]
struct PDirectory {
    key: String,
    #[serde(rename = "type")]
    kind: Option<String>,
}

#[derive(Deserialize)]
struct PHub {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PItem>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PItem {
    rating_key: String,
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    title: String,
    /// Bei Titeln: Künstler des Titels, falls er vom Album-Künstler abweicht.
    original_title: Option<String>,
    /// Bei Titeln: Album. Bei Alben: Künstler.
    parent_title: Option<String>,
    /// Bei Titeln: Album-Künstler.
    grandparent_title: Option<String>,
    parent_rating_key: Option<String>,
    grandparent_rating_key: Option<String>,
    thumb: Option<String>,
    parent_thumb: Option<String>,
    /// Nur bei Playlists: automatisch erzeugte Collage.
    composite: Option<String>,
    /// Titelnummer.
    index: Option<u32>,
    /// CD-Nummer.
    parent_index: Option<u32>,
    year: Option<u32>,
    parent_year: Option<u32>,
    /// Länge in Millisekunden (Titel, Playlists).
    duration: Option<u64>,
    /// Bewertung 0–10.
    user_rating: Option<f64>,
    view_count: Option<u32>,
    added_at: Option<i64>,
    last_viewed_at: Option<i64>,
    /// Anzahl der Titel (Albums, Playlists).
    leaf_count: Option<u32>,
    /// Anzahl der Unterelemente (bei Künstlern: Alben, falls geliefert).
    child_count: Option<u32>,
    #[serde(rename = "Media", default)]
    media: Vec<PMedia>,
    #[serde(rename = "Genre", default)]
    genre: Vec<PTag>,
}

#[derive(Deserialize, Default)]
struct PMedia {
    #[serde(rename = "Part", default)]
    part: Vec<PPart>,
}

#[derive(Deserialize, Default)]
struct PPart {
    key: Option<String>,
    #[serde(rename = "Stream", default)]
    stream: Vec<PStream>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PStream {
    /// 1 = Audio, 4 = Songtext.
    stream_type: Option<u32>,
    format: Option<String>,
    key: Option<String>,
}

#[derive(Deserialize)]
struct PTag {
    tag: String,
}

impl PItem {
    fn part_key(&self) -> Option<&str> {
        self.media.first()?.part.first()?.key.as_deref()
    }

    fn is_favorite(&self) -> bool {
        self.user_rating.is_some_and(|r| r >= FAVORITE_MIN_RATING)
    }

    fn duration_secs(&self) -> Option<u32> {
        self.duration.map(|ms| ((ms + 500) / 1000).min(u64::from(u32::MAX)) as u32)
    }

    /// Cover von Alben und Künstlern: ihr eigener `ratingKey`, wenn sie ein Bild haben.
    fn own_cover(&self) -> Option<String> {
        self.thumb.is_some().then(|| self.rating_key.clone())
    }
}

impl From<PItem> for Album {
    fn from(i: PItem) -> Self {
        Album {
            cover_art_id: i.own_cover(),
            starred: i.is_favorite(),
            artist: i.parent_title.clone(),
            artist_id: i.parent_rating_key.clone(),
            year: i.year,
            song_count: i.leaf_count,
            // Plex liefert für Alben keine Gesamtlänge.
            duration_secs: None,
            name: i.title,
            id: i.rating_key,
        }
    }
}

impl From<PItem> for Artist {
    fn from(i: PItem) -> Self {
        Artist {
            cover_art_id: i.own_cover(),
            starred: i.is_favorite(),
            album_count: i.child_count,
            name: i.title,
            id: i.rating_key,
        }
    }
}

impl From<PItem> for Playlist {
    fn from(i: PItem) -> Self {
        Playlist {
            cover_art_id: i.composite.is_some().then(|| format!("playlist:{}", i.rating_key)),
            duration_secs: i.duration_secs(),
            song_count: i.leaf_count,
            // Der Besitzer steht in der Antwort nicht.
            owner: None,
            name: i.title,
            id: i.rating_key,
        }
    }
}

impl From<PItem> for Track {
    fn from(i: PItem) -> Self {
        // Titel haben kaum ein eigenes Bild, sondern das ihres Albums.
        let cover_art_id = if i.parent_thumb.is_some() {
            i.parent_rating_key.clone()
        } else {
            i.own_cover()
        };
        let rating = i.user_rating.filter(|r| *r > 0.0);
        Track {
            cover_art_id,
            duration_secs: i.duration_secs(),
            // Bei Sammlern steht der Künstler des Titels in `originalTitle`.
            artist: i.original_title.clone().or_else(|| i.grandparent_title.clone()),
            artist_id: i.grandparent_rating_key.clone(),
            album: i.parent_title.clone(),
            album_id: i.parent_rating_key.clone(),
            track_number: i.index,
            disc_number: i.parent_index,
            year: i.parent_year.or(i.year),
            genre: i.genre.first().map(|g| g.tag.clone()),
            starred: i.is_favorite(),
            // Plex liefert keine ReplayGain-Werte über die API.
            replay_gain: None,
            user_rating: rating.map(|r| ((r / 2.0).round() as u32).clamp(1, 5)),
            play_count: i.view_count,
            added_at: i.added_at,
            title: i.title,
            id: i.rating_key,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> PlexClient {
        PlexClient::new(PlexCredentials {
            base_url: "http://plex.local:32400".into(),
            token: "TOK".into(),
            client_id: "cid".into(),
            sections: vec!["3".into()],
            machine_id: "MID".into(),
        })
        .unwrap()
    }

    fn parse(json: &str) -> Container {
        serde_json::from_str::<Root>(json).unwrap().container
    }

    const ALBUM_CHILDREN: &str = r#"{"MediaContainer":{"size":2,"totalSize":2,"Metadata":[
        {"ratingKey":"101","key":"/library/metadata/101","type":"track","title":"Bend",
         "grandparentTitle":"Binaerpilot","grandparentRatingKey":"7","parentTitle":"You Can't Stop Da Funk",
         "parentRatingKey":"55","parentThumb":"/library/metadata/55/thumb/1700000000","parentYear":2006,
         "index":1,"parentIndex":1,"duration":267546,"userRating":10.0,"viewCount":4,"addedAt":1574921233,
         "Media":[{"Part":[{"key":"/library/parts/900/1700000000/file.mp3"}]}]},
        {"ratingKey":"102","type":"track","title":"Zwei","originalTitle":"Gast","grandparentTitle":"Binaerpilot",
         "parentTitle":"You Can't Stop Da Funk","parentRatingKey":"55","index":2,"duration":1000,
         "userRating":6.0,"Genre":[{"tag":"Electronic"}]}]}}"#;

    #[test]
    fn parses_album_tracks() {
        let tracks: Vec<Track> = parse(ALBUM_CHILDREN).metadata.into_iter().map(Into::into).collect();
        assert_eq!(tracks.len(), 2);
        let t = &tracks[0];
        assert_eq!((t.id.as_str(), t.title.as_str()), ("101", "Bend"));
        assert_eq!(t.artist.as_deref(), Some("Binaerpilot"));
        assert_eq!(t.artist_id.as_deref(), Some("7"));
        assert_eq!(t.album_id.as_deref(), Some("55"));
        assert_eq!(t.cover_art_id.as_deref(), Some("55"));
        assert_eq!((t.track_number, t.disc_number, t.year), (Some(1), Some(1), Some(2006)));
        // 267546 ms -> gerundet 268 s.
        assert_eq!(t.duration_secs, Some(268));
        assert!(t.starred, "Bewertung 10 = Favorit");
        assert_eq!(t.user_rating, Some(5));
        assert_eq!(t.play_count, Some(4));
        assert_eq!(t.added_at, Some(1_574_921_233));
        // Zweiter Titel: Künstler des Titels, keine Favorit-Bewertung, Genre da, kein Cover.
        let t = &tracks[1];
        assert_eq!(t.artist.as_deref(), Some("Gast"));
        assert!(!t.starred);
        assert_eq!(t.user_rating, Some(3));
        assert_eq!(t.genre.as_deref(), Some("Electronic"));
        assert_eq!(t.cover_art_id, None);
    }

    #[test]
    fn parses_albums_artists_and_playlists() {
        let c = parse(
            r#"{"MediaContainer":{"totalSize":1,"Metadata":[
              {"ratingKey":"55","type":"album","title":"Album","parentTitle":"Künstler","parentRatingKey":"7",
               "thumb":"/library/metadata/55/thumb/1","year":2006,"leafCount":12,"addedAt":5,"userRating":9.0},
              {"ratingKey":"7","type":"artist","title":"Künstler","childCount":3},
              {"ratingKey":"900","type":"playlist","title":"Mix","leafCount":5,"duration":600000,
               "composite":"/playlists/900/composite/1","playlistType":"audio"}]}}"#,
        );
        let mut it = c.metadata.into_iter();
        let a: Album = it.next().unwrap().into();
        assert_eq!(a.artist.as_deref(), Some("Künstler"));
        assert_eq!(a.artist_id.as_deref(), Some("7"));
        assert_eq!((a.song_count, a.year), (Some(12), Some(2006)));
        assert_eq!(a.cover_art_id.as_deref(), Some("55"));
        assert!(a.starred);
        let ar: Artist = it.next().unwrap().into();
        // Künstler ohne Bild: kein Cover (statt einer 404-URL).
        assert_eq!((ar.cover_art_id, ar.album_count), (None, Some(3)));
        let p: Playlist = it.next().unwrap().into();
        assert_eq!((p.song_count, p.duration_secs), (Some(5), Some(600)));
        assert_eq!(p.cover_art_id.as_deref(), Some("playlist:900"));
    }

    #[test]
    fn parses_search_hubs() {
        let c = parse(
            r#"{"MediaContainer":{"size":3,"Hub":[
              {"type":"artist","title":"Artists","Metadata":[{"ratingKey":"7","type":"artist","title":"Binaerpilot"}]},
              {"type":"album","title":"Albums","Metadata":[{"ratingKey":"55","type":"album","title":"Funk","thumb":"/x"}]},
              {"type":"track","title":"Tracks","Metadata":[
                {"ratingKey":"101","type":"track","title":"Bend"},{"ratingKey":"102","type":"track","title":"Bend 2"}]},
              {"type":"playlist","title":"Playlists","Metadata":[{"ratingKey":"9","type":"playlist","title":"x"}]}]}}"#,
        );
        let mut r = SearchResult::default();
        for item in c.hub.into_iter().flat_map(|h| h.metadata) {
            match item.kind.as_deref() {
                Some("artist") => r.artists.push(item.into()),
                Some("album") => r.albums.push(item.into()),
                Some("track") => r.tracks.push(item.into()),
                _ => {}
            }
        }
        assert_eq!((r.artists.len(), r.albums.len(), r.tracks.len()), (1, 1, 2));
        assert_eq!(r.albums[0].cover_art_id.as_deref(), Some("55"));
        // Treffer ohne Ergebnisse: Hub fehlt ganz, das ist kein Fehler.
        assert!(parse(r#"{"MediaContainer":{"size":0}}"#).hub.is_empty());
    }

    #[test]
    fn finds_only_music_sections() {
        let c = parse(
            r#"{"MediaContainer":{"size":3,"Directory":[
              {"key":"1","type":"movie","title":"Filme"},
              {"key":"3","type":"artist","title":"Musik"},
              {"key":"5","type":"artist","title":"Hörbücher"}]}}"#,
        );
        assert_eq!(music_sections(&c), vec!["3", "5"]);
    }

    #[test]
    fn signin_success_and_errors() {
        let ok = br#"{"id":1,"username":"anna","authToken":"abc123","email":"a@b"}"#;
        assert_eq!(parse_signin(StatusCode::CREATED, ok).unwrap(), "abc123");

        let wrong = br#"{"errors":[{"code":1001,"message":"User could not be authenticated","status":401}]}"#;
        let err = parse_signin(StatusCode::UNAUTHORIZED, wrong).err().unwrap();
        // library::login erkennt "Passwort" und probiert dann keine zweite Adresse.
        assert!(err.to_string().contains("Passwort falsch"));

        let two_factor = br#"{"errors":[{"code":1029,"message":"Please enter the verification code","status":401}]}"#;
        let err = parse_signin(StatusCode::UNAUTHORIZED, two_factor).err().unwrap();
        assert!(err.to_string().contains("Zwei-Faktor"));

        let err = parse_signin(StatusCode::TOO_MANY_REQUESTS, b"").err().unwrap();
        assert!(err.to_string().contains("später"));
        // Keine JSON-Antwort bei Erfolg: klare Meldung statt Absturz.
        assert!(parse_signin(StatusCode::OK, b"<html>").is_err());
    }

    #[test]
    fn rejected_token_asks_for_new_login() {
        let e = status_error(StatusCode::UNAUTHORIZED).unwrap();
        // Das Wort "ungültig" erkennt library::login als "Zugangsdaten falsch".
        assert!(e.to_string().contains("ungültig"));
        assert!(status_error(StatusCode::OK).is_none());
    }

    #[test]
    fn base_url_is_normalized() {
        assert_eq!(normalize_base_url("plex.local:32400/"), "https://plex.local:32400");
        assert_eq!(
            normalize_base_url("http://192.168.1.5:32400/web/index.html#!/server/x"),
            "http://192.168.1.5:32400"
        );
    }

    #[test]
    fn stream_url_uses_file_when_known_else_transcoder() {
        let c = client();
        let orig = StreamQuality { format: AudioFormat::Original, bitrate_kbps: 0 };
        // Titel noch nie gesehen: Transcoder, Token und Titel-Pfad stecken drin.
        let before = c.stream_url("101", orig);
        assert!(before.starts_with("http://plex.local:32400/music/:/transcode/universal/start?"), "{before}");
        assert!(before.contains("X-Plex-Token=TOK") && before.contains("path=%2Flibrary%2Fmetadata%2F101"));
        assert!(before.contains("directStream=1"));

        // Nach dem Laden der Titelliste ist der Datei-Pfad bekannt.
        c.remember_parts(&parse(ALBUM_CHILDREN).metadata);
        let after = c.stream_url("101", orig);
        assert_eq!(
            after,
            "http://plex.local:32400/library/parts/900/1700000000/file.mp3?X-Plex-Token=TOK"
        );
    }

    #[test]
    fn stream_url_requests_transcoding() {
        let c = client();
        let u = c.stream_url("101", StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 96 });
        assert!(u.starts_with("http://plex.local:32400/music/:/transcode/universal/start.opus?"), "{u}");
        assert!(u.contains("audioBitrate=96") && u.contains("maxAudioBitrate=96"));
        assert!(u.contains("directPlay=0") && u.contains("directStream=0"));
        assert!(u.contains("X-Plex-Client-Identifier=cid") && u.contains("X-Plex-Token=TOK"));
        // Auch bei bekanntem Datei-Pfad darf eine Qualitätsstufe nicht die Originaldatei liefern.
        c.remember_parts(&parse(ALBUM_CHILDREN).metadata);
        let u = c.stream_url("101", StreamQuality { format: AudioFormat::Mp3, bitrate_kbps: 128 });
        assert!(u.contains("start.mp3") && u.contains("audioBitrate=128"));
    }

    #[test]
    fn cover_url_has_size_and_token() {
        let c = client();
        let u = c.cover_url("55", 300);
        assert!(u.starts_with("http://plex.local:32400/photo/:/transcode?"), "{u}");
        assert!(u.contains("url=%2Flibrary%2Fmetadata%2F55%2Fthumb"));
        assert!(u.contains("width=300") && u.contains("height=300") && u.contains("X-Plex-Token=TOK"));
        let pl = c.cover_url("playlist:900", 64);
        assert!(pl.contains("url=%2Fplaylists%2F900%2Fcomposite"));
    }

    #[test]
    fn paging_across_sections() {
        // Seite beginnt hinter Bibliothek 1 (3 Einträge): 3 überspringen, Rest bleibt.
        assert_eq!(advance_paging(5, 10, 0, 3), (2, 10));
        // Bibliothek lieferte 4 von 10: Rest kommt aus der nächsten, ab Anfang.
        assert_eq!(advance_paging(0, 10, 4, 4), (0, 6));
        assert_eq!(advance_paging(0, 10, 10, 50), (0, 0));
    }

    #[test]
    fn parses_lrc_lyrics() {
        let text = "[ar:Künstler]\n[00:10.50]Zeile 1\n[01:05]Zeile 2\n[00:20.5][02:00.00]Refrain\n";
        let l = parse_lrc(text).unwrap();
        assert!(l.synced);
        let times: Vec<_> = l.lines.iter().map(|x| x.start_ms.unwrap()).collect();
        assert_eq!(times, vec![10_500, 20_500, 65_000, 120_000]);
        assert_eq!(l.lines[1].text, "Refrain");
        // Ohne Stempel: normaler Text.
        let l = parse_lrc("Nur\nText\n").unwrap();
        assert!(!l.synced && l.lines.len() == 2 && l.lines[0].start_ms.is_none());
        assert!(parse_lrc("  \n").is_none());
    }

    #[test]
    fn credentials_roundtrip_without_secrets_beyond_token() {
        let json = serde_json::to_string(&client().credentials()).unwrap();
        assert!(json.contains("TOK") && !json.to_lowercase().contains("password"));
        // Ältere gespeicherte Sitzung ohne `sections`/`machine_id` bleibt lesbar.
        let c: PlexCredentials =
            serde_json::from_str(r#"{"base_url":"http://x","token":"t","client_id":"c"}"#).unwrap();
        assert!(c.sections.is_empty() && c.machine_id.is_empty());
    }
}
