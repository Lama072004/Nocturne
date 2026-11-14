//! Adapter für Jellyfin UND Emby.
//!
//! Doku: <https://api.jellyfin.org/> und <https://dev.emby.media/doc/restapi/>
//!
//! WARUM eine Datei für zwei Server? Jellyfin ist ein Fork von Emby. Pfade,
//! JSON-Felder (`Items`, `RunTimeTicks`, `UserData` …) und Anmeldung sind
//! praktisch gleich. Die Unterschiede sind klein und stecken in [`Flavor`]:
//!
//! | Unterschied               | Jellyfin                    | Emby                           |
//! |---------------------------|-----------------------------|--------------------------------|
//! | Pfad-Präfix               | keiner (`/Items`)           | `/emby` (`/emby/Items`)        |
//! | Anmelde-Header            | `Authorization`             | `X-Emby-Authorization`         |
//! | Token in URL-Parametern   | `ApiKey` + `api_key`        | `api_key` + `X-Emby-Token`     |
//! | Songtexte                 | `GET /Audio/{id}/Lyrics`    | keine vergleichbare Schnittstelle |
//! | Playlist anlegen          | JSON-Body                   | Query-Parameter                |
//! | Favorit / "gehört" setzen | neue + alte Route           | nur die alte Route             |
//!
//! # Anmeldung
//! `POST /Users/AuthenticateByName` mit Benutzername und Passwort liefert
//! eine UserId und ein AccessToken. Gespeichert werden NUR Server-URL, UserId,
//! Token und eine Geräte-ID, nie das Passwort. Das Token ist dauerhaft gültig,
//! bis man es am Server (Dashboard → Geräte) widerruft; dann liefern alle
//! Aufrufe 401, und die App fordert zur neuen Anmeldung auf.
//!
//! # Token in der URL
//! Cover- und Stream-URLs lädt ExoPlayer bzw. Flutter selbst und kann dabei
//! keine Header setzen. Deshalb hängt jede URL das Token als Query-Parameter
//! an. Ein aktueller Jellyfin (12.x, Demo-Server) akzeptiert dafür nur
//! `ApiKey`, ältere Versionen nur `api_key`: Wir senden beide, der Server
//! nimmt, was er kennt.
//!
//! # Was NICHT gegen einen echten Server geprüft ist
//! Jellyfin ist gegen den öffentlichen Demo-Server getestet (Live-Test unten,
//! nur lesend). Emby hat keinen öffentlichen Testserver: Dort sind nur die
//! Unterschiede aus der Tabelle nach Doku umgesetzt und per Unit-Test auf
//! korrekte URLs geprüft.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use rand::distributions::{Alphanumeric, DistString};
use reqwest::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use super::subsonic::parse_iso_utc;
use crate::api::models::{
    Album, AlbumDetail, Artist, ArtistDetail, LyricLine, Lyrics, Playlist, PlaylistDetail,
    ReplayGainInfo, SearchResult, ServerInfo, ServerKind, StarKind, Starred, Track,
};
use crate::api::settings::{AudioFormat, StreamQuality};

/// Name, unter dem die App im Server-Dashboard (Geräte) auftaucht.
const CLIENT_NAME: &str = "Nocturne";
const DEVICE_NAME: &str = "Android";
const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Welche Felder wir zusätzlich zu den Standardfeldern anfordern. Genres und
/// Datum kommen sonst nicht mit; `ChildCount` ist die Titelzahl eines Albums.
/// Bewusst nur Felder, die beide Server kennen: Ein unbekannter Wert in
/// `Fields` lässt Jellyfin mit HTTP 400 antworten.
const FIELDS: &str = "DateCreated,Genres,ChildCount";

/// So viele Titel pro Anfrage beim Schreiben von Playlists. 100 IDs à 32
/// Zeichen sind ~3,3 KB: Das schluckt jeder Proxy (viele kappen bei 8 KB).
const PLAYLIST_CHUNK: usize = 100;

/// Eine Tick-Einheit ist 100 Nanosekunden (so rechnet .NET Zeiten).
const TICKS_PER_SEC: u64 = 10_000_000;
const TICKS_PER_MS: i64 = 10_000;

/// Welche der beiden Server-Varianten gesprochen wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    Jellyfin,
    Emby,
}

impl Flavor {
    /// Emby hängt `/emby` vor jeden Pfad, Jellyfin nicht.
    fn path_prefix(self) -> Option<&'static str> {
        match self {
            Flavor::Jellyfin => None,
            Flavor::Emby => Some("emby"),
        }
    }

    /// Name des Headers, der beim Anmelden das Gerät beschreibt.
    fn auth_header(self) -> &'static str {
        match self {
            Flavor::Jellyfin => "Authorization",
            Flavor::Emby => "X-Emby-Authorization",
        }
    }

    pub fn kind(self) -> ServerKind {
        match self {
            Flavor::Jellyfin => ServerKind::Jellyfin,
            Flavor::Emby => ServerKind::Emby,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Flavor::Jellyfin => "Jellyfin",
            Flavor::Emby => "Emby",
        }
    }

    /// Hat der Server die neuen Routen (`/UserFavoriteItems/{id}`, …)? Nur
    /// Jellyfin ab 10.9. Emby kennt ausschließlich die alten.
    fn has_new_user_routes(self) -> bool {
        self == Flavor::Jellyfin
    }
}

/// Was gespeichert wird. Enthält KEIN Passwort.
///
/// Hinweis: Das Feld heißt absichtlich nicht `kind`, weil `SavedSession`
/// selbst ein Feld `kind` als Typ-Markierung in die JSON-Datei schreibt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JellyfinCredentials {
    /// Basis-URL ohne `/emby`, z. B. `https://media.example.org`
    pub base_url: String,
    pub user_id: String,
    pub username: String,
    pub access_token: String,
    /// Zufällige Kennung dieser App-Installation. Der Server verknüpft das
    /// Token mit ihr, und beim Umwandeln von Audio braucht er sie ebenfalls.
    pub device_id: String,
}

/// Nutzer tippen URLs sehr unterschiedlich ein. Wir machen daraus immer
/// `https://host[:port][/pfad]` ohne abschließenden Schrägstrich und ohne
/// Reste der Weboberfläche (`/web/index.html#!/home`), die man gern aus der
/// Adresszeile des Browsers kopiert.
fn normalize_base_url(input: &str, flavor: Flavor) -> String {
    let mut s = input.trim().to_owned();
    if let Some(i) = s.find('#') {
        s.truncate(i);
    }
    if !s.starts_with("http://") && !s.starts_with("https://") {
        // HTTPS als Standard: Bei HTTP gehen Token und Musik im Klartext übers Netz.
        s = format!("https://{s}");
    }
    // Von hinten abtragen, bis nichts mehr passt (z. B. ".../emby/web/index.html").
    let mut suffixes = vec!["/web/index.html", "/web", "/"];
    if flavor == Flavor::Emby {
        suffixes.push("/emby");
    }
    let scheme_len = if s.starts_with("https://") { 8 } else { 7 };
    loop {
        let lower = s.to_ascii_lowercase();
        match suffixes.iter().find(|suf| lower.ends_with(**suf)) {
            // Der Host darf nicht wegfallen: Schema und mindestens ein Zeichen bleiben stehen.
            Some(suf) if s.len() - suf.len() > scheme_len => s.truncate(s.len() - suf.len()),
            _ => break,
        }
    }
    s
}

/// Antwort von `POST /Users/AuthenticateByName` (nur, was wir brauchen).
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthResult {
    access_token: String,
    user: AuthUser,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AuthUser {
    id: String,
    name: Option<String>,
}

/// Wertet die Antwort der Anmeldung aus. Als eigene Funktion, damit sich die
/// Fehlermeldungen ohne Netzwerk testen lassen.
fn parse_login(flavor: Flavor, status: StatusCode, body: &[u8]) -> Result<AuthResult> {
    match status {
        s if s.is_success() => serde_json::from_slice(body)
            // 200 mit HTML heißt meist: Die Adresse zeigt auf eine Webseite
            // oder ein Captive-Portal, nicht auf den Server.
            .with_context(|| format!("Antwort stammt nicht von einem {}-Server", flavor.label())),
        // Beide Server antworten bei falschen Zugangsdaten mit 401 (Emby auch
        // mit 403 für gesperrte Konten).
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => bail!("Benutzername oder Passwort falsch"),
        StatusCode::NOT_FOUND => bail!(
            "Unter dieser Adresse läuft kein {}-Server (HTTP 404)",
            flavor.label()
        ),
        s => bail!("Anmeldung fehlgeschlagen (HTTP {})", s.as_u16()),
    }
}

/// Übersetzt einen Fehlerstatus in eine verständliche Meldung.
/// `None` bei Erfolg.
fn status_error(status: StatusCode) -> Option<anyhow::Error> {
    if status.is_success() {
        None
    } else if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        // Nach erfolgreicher Anmeldung heißt 401: Das Token wurde am Server
        // widerrufen. Die Meldung soll zur neuen Anmeldung führen.
        Some(anyhow!("Anmeldung abgelaufen – bitte neu anmelden (HTTP {})", status.as_u16()))
    } else {
        Some(anyhow!("Serverfehler (HTTP {})", status.as_u16()))
    }
}

/// Datum für "zuletzt gehört". Die beiden Server erwarten verschiedene
/// Schreibweisen: Jellyfin ISO 8601, Emby `yyyyMMddHHmmss`.
fn format_play_date(flavor: Flavor, unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let rest = unix_secs.rem_euclid(86_400);
    let (h, mi, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    // "civil from days": Umkehrung der Formel aus subsonic::parse_iso_utc.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    match flavor {
        Flavor::Jellyfin => format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z"),
        Flavor::Emby => format!("{y:04}{m:02}{d:02}{h:02}{mi:02}{s:02}"),
    }
}

fn new_device_id() -> String {
    format!("nocturne-{}", Alphanumeric.sample_string(&mut rand::thread_rng(), 16))
}

fn build_http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        // Lieber früh einen klaren Fehler anzeigen, als ewig zu hängen,
        // wenn der Server zu Hause nicht erreichbar ist.
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(30))
        .build()?)
}

pub struct JellyfinClient {
    flavor: Flavor,
    creds: JellyfinCredentials,
    http: reqwest::Client,
}

impl JellyfinClient {
    pub fn new(flavor: Flavor, creds: JellyfinCredentials) -> Result<Self> {
        Ok(Self { flavor, creds, http: build_http()? })
    }

    pub fn credentials(&self) -> &JellyfinCredentials {
        &self.creds
    }

    /// Meldet sich mit Benutzername und Passwort an.
    ///
    /// Anders als bei Subsonic lässt sich das Token nicht lokal berechnen:
    /// Der Server stellt es aus, deshalb ist diese Funktion `async`. Das
    /// Passwort verlässt sie nicht und wird nirgends gespeichert.
    pub async fn login(flavor: Flavor, base_url: &str, username: &str, password: &str) -> Result<Self> {
        let base_url = normalize_base_url(base_url, flavor);
        let device_id = new_device_id();
        let http = build_http()?;

        let mut url = Url::parse(&base_url).context("Ungültige Server-Adresse")?;
        {
            let mut p = url.path_segments_mut().map_err(|_| anyhow!("Ungültige Server-Adresse"))?;
            p.pop_if_empty();
            if let Some(prefix) = flavor.path_prefix() {
                p.push(prefix);
            }
            p.extend(["Users", "AuthenticateByName"]);
        }
        let header = format!(
            "MediaBrowser Client=\"{CLIENT_NAME}\", Device=\"{DEVICE_NAME}\", DeviceId=\"{device_id}\", Version=\"{CLIENT_VERSION}\""
        );
        let resp = http
            .post(url)
            .header(flavor.auth_header(), header)
            // `Pw` ist der Klartext-Name des Felds bei beiden Servern. Das
            // Passwort geht nur in diesem einen Aufruf über das Netz.
            .json(&json!({ "Username": username, "Pw": password }))
            .send()
            .await
            .context("Server nicht erreichbar")?;
        let status = resp.status();
        let body = resp.bytes().await?;
        let auth = parse_login(flavor, status, &body)?;

        let creds = JellyfinCredentials {
            base_url,
            user_id: auth.user.id,
            username: auth.user.name.unwrap_or_else(|| username.to_owned()),
            access_token: auth.access_token,
            device_id,
        };
        Ok(Self { flavor, creds, http })
    }

    // -----------------------------------------------------------------------
    // HTTP-Grundlagen
    // -----------------------------------------------------------------------

    /// Baut eine vollständige URL inkl. Anmeldung (siehe "Token in der URL"
    /// oben). Pfadteile werden einzeln übergeben und dabei prozentkodiert,
    /// damit eine Server-ID nie die Adresse "umbauen" kann.
    fn url(&self, segments: &[&str], params: &[(&str, &str)]) -> String {
        let mut url = Url::parse(&self.creds.base_url)
            .unwrap_or_else(|_| Url::parse("https://invalid.invalid").unwrap());
        if let Ok(mut p) = url.path_segments_mut() {
            p.pop_if_empty();
            if let Some(prefix) = self.flavor.path_prefix() {
                p.push(prefix);
            }
            p.extend(segments);
        }
        {
            let mut q = url.query_pairs_mut();
            match self.flavor {
                Flavor::Jellyfin => {
                    q.append_pair("ApiKey", &self.creds.access_token);
                    q.append_pair("api_key", &self.creds.access_token);
                }
                Flavor::Emby => {
                    q.append_pair("api_key", &self.creds.access_token);
                    q.append_pair("X-Emby-Token", &self.creds.access_token);
                }
            }
            for (k, v) in params {
                q.append_pair(k, v);
            }
        }
        url.into()
    }

    /// Schickt eine Anfrage und gibt die Antwort zurück, OHNE den Status zu
    /// prüfen (für Aufrufe, die bei 404 etwas anderes versuchen).
    async fn send_raw(
        &self,
        method: Method,
        segments: &[&str],
        params: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        let mut req = self.http.request(method, self.url(segments, params));
        if let Some(b) = body {
            req = req.json(&b);
        }
        req.send()
            .await
            .with_context(|| format!("Server nicht erreichbar ({})", segments.join("/")))
    }

    /// Wie `send_raw`, aber mit Statusprüfung.
    async fn send(
        &self,
        method: Method,
        segments: &[&str],
        params: &[(&str, &str)],
        body: Option<serde_json::Value>,
    ) -> Result<reqwest::Response> {
        let resp = self.send_raw(method, segments, params, body).await?;
        match status_error(resp.status()) {
            Some(e) => Err(e),
            None => Ok(resp),
        }
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, segments: &[&str], params: &[(&str, &str)]) -> Result<T> {
        let bytes = self.send(Method::GET, segments, params, None).await?.bytes().await?;
        serde_json::from_slice(&bytes).context("Unerwartete Antwort vom Server")
    }

    /// Eine Aktion, die es in neueren Jellyfin-Versionen unter einer neuen
    /// Route gibt (`new`) und überall unter der alten (`legacy`). Zuerst die
    /// neue probieren, bei "nicht gefunden" die alte. Emby kennt nur die alte.
    ///
    /// WARUM beide? Die alte Route ist bei Jellyfin als veraltet markiert und
    /// könnte irgendwann verschwinden; ältere Server kennen die neue nicht.
    async fn send_with_fallback(
        &self,
        method: Method,
        new: &[&str],
        legacy: &[&str],
        params: &[(&str, &str)],
    ) -> Result<()> {
        if self.flavor.has_new_user_routes() {
            let resp = self.send_raw(method.clone(), new, params, None).await?;
            if resp.status() != StatusCode::NOT_FOUND && resp.status() != StatusCode::METHOD_NOT_ALLOWED {
                return match status_error(resp.status()) {
                    Some(e) => Err(e),
                    None => Ok(()),
                };
            }
        }
        // Die alte Route trägt den Benutzer im Pfad, `userId` im Query stört nicht.
        self.send(method, legacy, params, None).await?;
        Ok(())
    }

    /// Abfrage auf `/Items` mit den immer gleichen Grundparametern.
    async fn items(&self, extra: &[(&str, &str)]) -> Result<Vec<JItem>> {
        self.list(&["Items"], extra).await
    }

    /// Listenabfrage (`/Items`, `/Artists`, `/Playlists/{id}/Items` …).
    async fn list(&self, segments: &[&str], extra: &[(&str, &str)]) -> Result<Vec<JItem>> {
        let mut params: Vec<(&str, &str)> = vec![
            ("userId", &self.creds.user_id),
            ("Fields", FIELDS),
            ("EnableUserData", "true"),
            // Nur das Titelbild: spart pro Eintrag die Liste aller Bildarten.
            ("EnableImageTypes", "Primary"),
            ("ImageTypeLimit", "1"),
        ];
        params.extend_from_slice(extra);
        let r: ItemsResponse = self.get(segments, &params).await?;
        Ok(r.items)
    }

    /// Genau ein Eintrag per ID. `/Items?Ids=` statt `/Items/{id}`, weil die
    /// Listenabfrage bei allen Versionen beider Server gleich funktioniert.
    async fn item(&self, id: &str) -> Result<JItem> {
        self.items(&[("Ids", id)])
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Eintrag nicht gefunden"))
    }

    // -----------------------------------------------------------------------
    // Schnittstelle von ServerClient
    // -----------------------------------------------------------------------

    pub async fn ping(&self) -> Result<ServerInfo> {
        // Öffentlicher Endpunkt: Name und Version, keine Anmeldung nötig.
        let info: PublicInfo = self.get(&["System", "Info", "Public"], &[]).await?;
        // Eigener Aufruf, der das Token prüft: `System/Info/Public` antwortet
        // auch mit abgelaufenem Token, und die App soll es merken.
        self.send(Method::GET, &["Users", &self.creds.user_id], &[], None).await?;
        Ok(ServerInfo {
            kind: self.flavor.kind(),
            server_type: info.product_name.or_else(|| Some(self.flavor.label().to_owned())),
            server_version: info.version.clone(),
            api_version: info.version.unwrap_or_default(),
            // Gibt es nur bei Subsonic-Servern.
            open_subsonic: false,
        })
    }

    /// Zuletzt hinzugefügte Alben.
    pub async fn newest_albums(&self, count: u32) -> Result<Vec<Album>> {
        let limit = count.to_string();
        let items = self
            .items(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "MusicAlbum"),
                ("SortBy", "DateCreated"),
                ("SortOrder", "Descending"),
                ("Limit", &limit),
            ])
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Zuletzt gehörte Alben. Der Server merkt sich die Wiedergabe pro
    /// Titel; für Alben fragt `SortBy=DatePlayed` mit `Filters=IsPlayed`
    /// deshalb nach Alben, die als gehört gelten.
    pub async fn recent_albums(&self, count: u32) -> Result<Vec<Album>> {
        let limit = count.to_string();
        let items = self
            .items(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "MusicAlbum"),
                ("SortBy", "DatePlayed"),
                ("SortOrder", "Descending"),
                ("Filters", "IsPlayed"),
                ("Limit", &limit),
            ])
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Alle Alben alphabetisch, seitenweise.
    pub async fn albums_page(&self, offset: u32, size: u32) -> Result<Vec<Album>> {
        let (start, limit) = (offset.to_string(), size.to_string());
        let items = self
            .items(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "MusicAlbum"),
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"),
                ("StartIndex", &start),
                ("Limit", &limit),
            ])
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn album(&self, id: &str) -> Result<AlbumDetail> {
        let album = self.item(id).await?;
        let tracks = self
            .items(&[
                ("ParentId", id),
                ("Recursive", "true"),
                ("IncludeItemTypes", "Audio"),
                // Erst nach CD, dann nach Titelnummer: so wie auf dem Album.
                ("SortBy", "ParentIndexNumber,IndexNumber,SortName"),
            ])
            .await?;
        Ok(AlbumDetail {
            album: album.into(),
            tracks: tracks.into_iter().map(Into::into).collect(),
        })
    }

    /// Alle Album-Künstler, alphabetisch.
    ///
    /// `AlbumArtists` statt `Artists`: Letzteres enthält jeden, der je auf
    /// einem Titel mitsingt (Gäste, Komponisten) und bläht die Liste auf.
    pub async fn artists(&self) -> Result<Vec<Artist>> {
        // `ItemCounts` liefert die Albumzahl. Nur Jellyfin ist damit geprüft;
        // bei Emby lassen wir es weg, bevor ein unbekannter Wert Fehler wirft.
        let fields = format!("{FIELDS},ItemCounts");
        let mut extra = vec![("SortBy", "SortName"), ("SortOrder", "Ascending")];
        if self.flavor == Flavor::Jellyfin {
            extra.push(("Fields", &fields));
        }
        let items = self.list_artists(&["Artists", "AlbumArtists"], &extra).await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Wie `list`, aber `Fields` aus `extra` ersetzt den Standard.
    async fn list_artists(&self, segments: &[&str], extra: &[(&str, &str)]) -> Result<Vec<JItem>> {
        let mut params: Vec<(&str, &str)> = vec![
            ("userId", &self.creds.user_id),
            ("EnableUserData", "true"),
            ("EnableImageTypes", "Primary"),
            ("ImageTypeLimit", "1"),
        ];
        if !extra.iter().any(|(k, _)| *k == "Fields") {
            params.push(("Fields", FIELDS));
        }
        params.extend_from_slice(extra);
        let r: ItemsResponse = self.get(segments, &params).await?;
        Ok(r.items)
    }

    pub async fn artist(&self, id: &str) -> Result<ArtistDetail> {
        let artist = self.item(id).await?;
        let sort = [
            ("Recursive", "true"),
            ("IncludeItemTypes", "MusicAlbum"),
            ("SortBy", "ProductionYear,SortName"),
            ("SortOrder", "Descending"),
        ];
        // Erst Alben, bei denen er Album-Künstler ist; leer (z. B. nur als
        // Gast auf Sammlern) -> alle, auf denen er vorkommt.
        let mut params = vec![("AlbumArtistIds", id)];
        params.extend_from_slice(&sort);
        let mut albums = self.items(&params).await?;
        if albums.is_empty() {
            let mut params = vec![("ArtistIds", id)];
            params.extend_from_slice(&sort);
            albums = self.items(&params).await?;
        }
        if albums.is_empty() {
            // Letzter Ausweg: Viele Sammlungen haben Dateien, bei denen nur
            // der Titel einen Künstler trägt, das Album aber keinen (so auf
            // dem Demo-Server). Dann über die Titel gehen und deren Alben
            // einsammeln.
            let tracks = self
                .items(&[("ArtistIds", id), ("Recursive", "true"), ("IncludeItemTypes", "Audio")])
                .await?;
            let mut album_ids: Vec<&str> = Vec::new();
            for album_id in tracks.iter().filter_map(|t| t.album_id.as_deref()) {
                if !album_ids.contains(&album_id) {
                    album_ids.push(album_id);
                }
            }
            if !album_ids.is_empty() {
                albums = self.items(&[("Ids", &album_ids.join(","))]).await?;
            }
        }
        Ok(ArtistDetail {
            artist: artist.into(),
            albums: albums.into_iter().map(Into::into).collect(),
        })
    }

    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let items = self
            .items(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "Playlist"),
                // Es gibt auch Video-Playlists; die interessieren hier nicht.
                ("MediaTypes", "Audio"),
                ("SortBy", "SortName"),
            ])
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    pub async fn playlist(&self, id: &str) -> Result<PlaylistDetail> {
        let playlist = self.item(id).await?;
        let tracks = self.list(&["Playlists", id, "Items"], &[]).await?;
        Ok(PlaylistDetail {
            playlist: playlist.into(),
            tracks: tracks.into_iter().map(Into::into).collect(),
        })
    }

    /// Legt eine Playlist an und gibt ihre ID zurück.
    ///
    /// Beim Anlegen geht nur ein Teil der Titel mit, der Rest folgt per
    /// `append_to_playlist`: Lange Listen würden die Adresse sprengen (Emby
    /// nimmt die IDs im Query, siehe Tabelle oben).
    async fn create_playlist_id(&self, name: &str, song_ids: &[String]) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Created {
            id: String,
        }
        let first = &song_ids[..song_ids.len().min(PLAYLIST_CHUNK)];
        let resp = match self.flavor {
            Flavor::Jellyfin => {
                // Seit 10.8 ist der JSON-Body der vorgesehene Weg.
                let body = json!({
                    "Name": name,
                    "Ids": first,
                    "UserId": self.creds.user_id,
                    "MediaType": "Audio",
                });
                self.send(Method::POST, &["Playlists"], &[], Some(body)).await?
            }
            Flavor::Emby => {
                let ids = first.join(",");
                self.send(
                    Method::POST,
                    &["Playlists"],
                    &[
                        ("Name", name),
                        ("Ids", &ids),
                        ("UserId", &self.creds.user_id),
                        ("MediaType", "Audio"),
                    ],
                    None,
                )
                .await?
            }
        };
        let created: Created = serde_json::from_slice(&resp.bytes().await?)
            .context("Server hat die Playlist-ID nicht mitgeteilt")?;
        self.append_to_playlist(&created.id, &song_ids[first.len()..]).await?;
        Ok(created.id)
    }

    async fn append_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        for chunk in song_ids.chunks(PLAYLIST_CHUNK) {
            let ids = chunk.join(",");
            self.send(
                Method::POST,
                &["Playlists", playlist_id, "Items"],
                &[("Ids", &ids), ("UserId", &self.creds.user_id)],
                None,
            )
            .await?;
        }
        Ok(())
    }

    pub async fn create_playlist(&self, name: &str, song_ids: &[String]) -> Result<()> {
        self.create_playlist_id(name, song_ids).await.map(|_| ())
    }

    pub async fn add_to_playlist(&self, playlist_id: &str, song_ids: &[String]) -> Result<()> {
        self.append_to_playlist(playlist_id, song_ids).await
    }

    /// Alle Favoriten. Drei Abfragen, weil Titel, Alben und Künstler an
    /// verschiedenen Stellen liegen.
    pub async fn starred(&self) -> Result<Starred> {
        // `Filters` ist der alte Name, `IsFavorite` der neue. Beide zu senden
        // ist harmlos und deckt alle Versionen ab.
        let fav = [("Filters", "IsFavorite"), ("IsFavorite", "true"), ("Recursive", "true")];
        let mut tracks_q = vec![("IncludeItemTypes", "Audio")];
        tracks_q.extend_from_slice(&fav);
        let mut albums_q = vec![("IncludeItemTypes", "MusicAlbum")];
        albums_q.extend_from_slice(&fav);
        let tracks = self.items(&tracks_q).await?;
        let albums = self.items(&albums_q).await?;
        let artists = self.list(&["Artists"], &fav).await?;
        Ok(Starred {
            artists: artists.into_iter().map(Into::into).collect(),
            albums: albums.into_iter().map(Into::into).collect(),
            tracks: tracks.into_iter().map(Into::into).collect(),
        })
    }

    /// Favorit setzen oder entfernen. Gilt für Titel, Alben und Künstler
    /// gleich: Der Server unterscheidet sie an der ID, nicht an der Route.
    pub async fn set_starred(&self, _kind: StarKind, id: &str, starred: bool) -> Result<()> {
        let method = if starred { Method::POST } else { Method::DELETE };
        self.send_with_fallback(
            method,
            &["UserFavoriteItems", id],
            &["Users", &self.creds.user_id, "FavoriteItems", id],
            &[("userId", &self.creds.user_id)],
        )
        .await
    }

    /// Meldet dem Server eine Wiedergabe.
    ///
    /// `submission = true`: Titel wurde gehört, der Server erhöht den
    /// Wiedergabezähler. Mit `time_ms` bekommt er den Zeitpunkt des Hörens,
    /// damit offline Gehörtes mit dem richtigen Datum landet. Bei Emby ist
    /// das Datumsformat nur nach Doku umgesetzt.
    ///
    /// `submission = false`: "läuft gerade". Der Server zeigt den Titel dann
    /// unter "Aktive Geräte" an. Wir melden bewusst nur den Start und keine
    /// Fortschrittsmeldungen: Das hielte den Funk wach (Akku). Der Eintrag
    /// verschwindet beim Server nach einiger Zeit von selbst.
    pub async fn scrobble(&self, song_id: &str, time_ms: Option<i64>, submission: bool) -> Result<()> {
        if submission {
            let date = time_ms.map(|ms| format_play_date(self.flavor, ms.div_euclid(1000)));
            let mut params: Vec<(&str, &str)> = vec![("userId", &self.creds.user_id)];
            if let Some(d) = &date {
                params.push(("datePlayed", d));
            }
            self.send_with_fallback(
                Method::POST,
                &["UserPlayedItems", song_id],
                &["Users", &self.creds.user_id, "PlayedItems", song_id],
                &params,
            )
            .await
        } else {
            self.send(
                Method::POST,
                &["Sessions", "Playing"],
                &[],
                Some(json!({
                    "ItemId": song_id,
                    "CanSeek": true,
                    "PositionTicks": 0,
                    "PlayMethod": "DirectStream",
                })),
            )
            .await?;
            Ok(())
        }
    }

    /// Songtext über `GET /Audio/{id}/Lyrics` (Jellyfin ab 10.9). Fehlt der
    /// Text, antwortet der Server mit 404: Das ist für den Nutzer dasselbe
    /// wie "kein Text", also `None` statt Fehler.
    ///
    /// Emby hat keine vergleichbare Schnittstelle, deshalb dort immer `None`.
    pub async fn lyrics(&self, song_id: &str) -> Result<Option<Lyrics>> {
        if self.flavor == Flavor::Emby {
            return Ok(None);
        }
        let Ok(resp) = self.send(Method::GET, &["Audio", song_id, "Lyrics"], &[], None).await else {
            return Ok(None);
        };
        Ok(parse_lyrics(&resp.bytes().await?))
    }

    /// Suche über Titel, Alben und Künstler. Drei Anfragen nacheinander, weil
    /// die Server keine gemischte Suche mit getrennten Obergrenzen kennen.
    /// (Nacheinander statt parallel: Das Makro dafür bräuchte ein weiteres
    /// Tokio-Feature, und die Anfragen sind im LAN ohnehin schnell.)
    pub async fn search(&self, query: &str) -> Result<SearchResult> {
        let q = [("SearchTerm", query), ("Recursive", "true")];
        let mut tracks_q = vec![("IncludeItemTypes", "Audio"), ("Limit", "50")];
        tracks_q.extend_from_slice(&q);
        let mut albums_q = vec![("IncludeItemTypes", "MusicAlbum"), ("Limit", "20")];
        albums_q.extend_from_slice(&q);
        let mut artists_q = vec![("Limit", "10")];
        artists_q.extend_from_slice(&q);
        let artists = self.list(&["Artists"], &artists_q).await?;
        let albums = self.items(&albums_q).await?;
        let tracks = self.items(&tracks_q).await?;
        Ok(build_search(artists, albums, tracks))
    }

    /// Alle Titel der Bibliothek, seitenweise (für die lokale Kopie, auf der
    /// die Smart-Playlists rechnen).
    ///
    /// WARUM diese Sortierung? Beim seitenweisen Laden darf die Reihenfolge
    /// zwischen zwei Anfragen nicht "wackeln", sonst fehlen Titel oder
    /// kommen doppelt. Mehrere Sortierschlüssel machen sie eindeutig.
    pub async fn songs_page(&self, offset: u32, size: u32) -> Result<Vec<Track>> {
        let (start, limit) = (offset.to_string(), size.to_string());
        let items = self
            .items(&[
                ("Recursive", "true"),
                ("IncludeItemTypes", "Audio"),
                ("SortBy", "AlbumArtist,Album,ParentIndexNumber,IndexNumber,SortName"),
                ("StartIndex", &start),
                ("Limit", &limit),
            ])
            .await?;
        Ok(items.into_iter().map(Into::into).collect())
    }

    /// Schreibt eine Playlist komplett neu (für gespiegelte Smart-Playlists).
    ///
    /// Mit `existing_id` wird der Inhalt ersetzt: erst alle Einträge löschen,
    /// dann die neuen anfügen. Gibt es die Playlist nicht mehr (am Server
    /// gelöscht), legen wir sie neu an. Gibt die ID der Playlist zurück.
    pub async fn write_playlist(
        &self,
        existing_id: Option<&str>,
        name: &str,
        song_ids: &[String],
    ) -> Result<Option<String>> {
        if let Some(id) = existing_id {
            if self.replace_playlist_items(id, song_ids).await.is_ok() {
                return Ok(Some(id.to_owned()));
            }
        }
        self.create_playlist_id(name, song_ids).await.map(Some)
    }

    async fn replace_playlist_items(&self, id: &str, song_ids: &[String]) -> Result<()> {
        // Gelöscht wird über die `PlaylistItemId` des Eintrags, nicht über die
        // Titel-ID: Dieselbe Titel-ID kann mehrfach in einer Playlist stehen.
        let current = self.list(&["Playlists", id, "Items"], &[]).await?;
        let entries: Vec<&str> = current.iter().filter_map(|i| i.playlist_item_id.as_deref()).collect();
        for chunk in entries.chunks(PLAYLIST_CHUNK) {
            let joined = chunk.join(",");
            self.send(Method::DELETE, &["Playlists", id, "Items"], &[("EntryIds", &joined)], None).await?;
        }
        self.append_to_playlist(id, song_ids).await
    }

    /// Cover-URL in passender Größe.
    ///
    /// AKKU/DATEN: Immer die tatsächlich angezeigte Größe anfordern. Der
    /// Server verkleinert und spart so Datenvolumen und Rechenzeit auf dem
    /// Handy. `cover_id` ist die ID des Albums (bzw. Künstlers/Playlist).
    pub fn cover_url(&self, cover_id: &str, size: u32) -> String {
        let size = size.to_string();
        self.url(
            &["Items", cover_id, "Images", "Primary"],
            &[("maxWidth", &size), ("maxHeight", &size), ("quality", "90")],
        )
    }

    /// Stream-URL in gewünschter Qualität.
    ///
    /// Original: Die Datei kommt unverändert (`static=true`), Springen im
    /// Titel funktioniert über Range-Anfragen. Sonst wandelt der Server mit
    /// ffmpeg um; die Endung im Pfad legt den Container fest, `audioCodec`
    /// und `audioBitRate` (in Bit/s!) das Format. Umgewandelte Streams
    /// erlauben kein Springen im Titel, bis der Server so weit ist.
    pub fn stream_url(&self, song_id: &str, q: StreamQuality) -> String {
        let device = self.creds.device_id.as_str();
        match q.format {
            AudioFormat::Original => {
                self.url(&["Audio", song_id, "stream"], &[("static", "true"), ("deviceId", device)])
            }
            other => {
                let (ext, codec) = match other {
                    AudioFormat::Opus => ("opus", "opus"),
                    AudioFormat::Aac => ("aac", "aac"),
                    AudioFormat::Mp3 => ("mp3", "mp3"),
                    AudioFormat::Original => unreachable!(),
                };
                let bits = (q.bitrate_kbps.max(32) * 1000).to_string();
                let file = format!("stream.{ext}");
                self.url(
                    &["Audio", song_id, &file],
                    &[("audioCodec", codec), ("audioBitRate", &bits), ("deviceId", device)],
                )
            }
        }
    }
}

/// Baut das Suchergebnis aus den drei Teilantworten.
fn build_search(artists: Vec<JItem>, albums: Vec<JItem>, tracks: Vec<JItem>) -> SearchResult {
    SearchResult {
        artists: artists.into_iter().map(Into::into).collect(),
        albums: albums.into_iter().map(Into::into).collect(),
        tracks: tracks.into_iter().map(Into::into).collect(),
    }
}

/// Wertet die Antwort von `/Audio/{id}/Lyrics` aus.
///
/// Zeiten kommen in Ticks (100 ns). Hat keine Zeile einen Zeitstempel, ist
/// der Text nicht synchronisiert (`start_ms = None`).
fn parse_lyrics(body: &[u8]) -> Option<Lyrics> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Dto {
        #[serde(default)]
        metadata: Option<Meta>,
        #[serde(default)]
        lyrics: Vec<Line>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Meta {
        is_synced: Option<bool>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Line {
        #[serde(default)]
        text: String,
        start: Option<i64>,
    }
    let dto: Dto = serde_json::from_slice(body).ok()?;
    if dto.lyrics.is_empty() {
        return None;
    }
    let any_time = dto.lyrics.iter().any(|l| l.start.is_some());
    let synced = dto.metadata.and_then(|m| m.is_synced).unwrap_or(any_time) && any_time;
    Some(Lyrics {
        synced,
        lines: dto
            .lyrics
            .into_iter()
            .map(|l| LyricLine {
                start_ms: if synced {
                    l.start.map(|t| (t / TICKS_PER_MS).clamp(0, i64::from(u32::MAX)) as u32)
                } else {
                    None
                },
                text: l.text,
            })
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// JSON-Strukturen (nur intern; nach außen gehen api::models)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PublicInfo {
    product_name: Option<String>,
    version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemsResponse {
    #[serde(default)]
    items: Vec<JItem>,
}

/// Ein Eintrag der Bibliothek. Jellyfin/Emby liefern für Titel, Alben,
/// Künstler und Playlists dieselbe Struktur (`BaseItemDto`); wir lesen sie
/// in einem Typ ein und wandeln danach je nach Ziel um.
#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct JItem {
    id: String,
    name: Option<String>,
    /// Alle beteiligten Künstler des Titels (Anzeigenamen).
    #[serde(default)]
    artists: Vec<String>,
    #[serde(default)]
    artist_items: Vec<NameId>,
    album: Option<String>,
    album_id: Option<String>,
    album_artist: Option<String>,
    #[serde(default)]
    album_artists: Vec<NameId>,
    run_time_ticks: Option<u64>,
    /// Titelnummer.
    index_number: Option<u32>,
    /// CD-Nummer.
    parent_index_number: Option<u32>,
    production_year: Option<u32>,
    #[serde(default)]
    genres: Vec<String>,
    user_data: Option<UserData>,
    date_created: Option<String>,
    /// Vorhandene Bildarten (Name -> Prüfsumme). Uns reicht, ob "Primary" da ist.
    #[serde(default)]
    image_tags: std::collections::HashMap<String, String>,
    /// Gesetzt, wenn das ALBUM ein Bild hat (Titel haben selten ein eigenes).
    album_primary_image_tag: Option<String>,
    /// Bei Alben: Anzahl der Titel; bei Playlists: Länge.
    child_count: Option<u32>,
    /// Nur bei Künstlern und nur mit `ItemCounts` (siehe `artists`).
    album_count: Option<u32>,
    /// Lautstärke-Anpassung in dB, die Jellyfin beim Scannen berechnet.
    /// Sie zielt auf -18 LUFS und entspricht damit dem ReplayGain-2-Wert.
    normalization_gain: Option<f32>,
    /// ID des Eintrags in einer Playlist (zum Löschen nötig).
    playlist_item_id: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct NameId {
    id: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct UserData {
    #[serde(default)]
    is_favorite: bool,
    play_count: Option<u32>,
    /// Bewertung 0–10 (Jellyfin) – bei uns 1–5 Sterne.
    rating: Option<f64>,
}

impl JItem {
    fn has_primary_image(&self) -> bool {
        self.image_tags.contains_key("Primary")
    }

    fn is_favorite(&self) -> bool {
        self.user_data.as_ref().is_some_and(|u| u.is_favorite)
    }

    fn duration_secs(&self) -> Option<u32> {
        // Auf ganze Sekunden runden wie die Zeitanzeige im Player.
        self.run_time_ticks.map(|t| ((t + TICKS_PER_SEC / 2) / TICKS_PER_SEC).min(u64::from(u32::MAX)) as u32)
    }

    /// Cover für Alben, Künstler und Playlists: das eigene Bild, wenn es eins gibt.
    fn own_cover(&self) -> Option<String> {
        self.has_primary_image().then(|| self.id.clone())
    }
}

impl From<JItem> for Album {
    fn from(i: JItem) -> Self {
        Album {
            cover_art_id: i.own_cover(),
            starred: i.is_favorite(),
            duration_secs: i.duration_secs(),
            artist: i.album_artist.clone().or_else(|| i.artists.first().cloned()),
            artist_id: i.album_artists.first().or(i.artist_items.first()).map(|a| a.id.clone()),
            year: i.production_year,
            song_count: i.child_count,
            name: i.name.unwrap_or_default(),
            id: i.id,
        }
    }
}

impl From<JItem> for Artist {
    fn from(i: JItem) -> Self {
        Artist {
            cover_art_id: i.own_cover(),
            starred: i.is_favorite(),
            // 0 ist auf dem Demo-Server auch bei Künstlern mit Album zu sehen
            // (Zähler nicht immer berechnet) – dann lieber "unbekannt".
            album_count: i.album_count.filter(|n| *n > 0),
            name: i.name.unwrap_or_default(),
            id: i.id,
        }
    }
}

impl From<JItem> for Playlist {
    fn from(i: JItem) -> Self {
        Playlist {
            cover_art_id: i.own_cover(),
            duration_secs: i.duration_secs(),
            song_count: i.child_count,
            // Der Besitzer steht in der Antwort nicht direkt.
            owner: None,
            name: i.name.unwrap_or_default(),
            id: i.id,
        }
    }
}

impl From<JItem> for Track {
    fn from(i: JItem) -> Self {
        // Cover: Titel haben fast nie ein eigenes Bild, sondern das ihres
        // Albums. Fehlt auch das, bleibt es leer (die UI zeigt Platzhalter).
        let cover_art_id = match (&i.album_primary_image_tag, &i.album_id) {
            (Some(_), Some(album_id)) => Some(album_id.clone()),
            _ => i.own_cover(),
        };
        let rating = i.user_data.as_ref().and_then(|u| u.rating).filter(|r| *r > 0.0);
        Track {
            cover_art_id,
            duration_secs: i.duration_secs(),
            // Mehrere Künstler als "A, B" anzeigen; ohne Angabe der Album-Künstler.
            artist: if i.artists.is_empty() { i.album_artist.clone() } else { Some(i.artists.join(", ")) },
            artist_id: i.artist_items.first().or(i.album_artists.first()).map(|a| a.id.clone()),
            track_number: i.index_number,
            disc_number: i.parent_index_number,
            year: i.production_year,
            genre: i.genres.first().cloned(),
            starred: i.is_favorite(),
            replay_gain: i.normalization_gain.map(|g| ReplayGainInfo {
                track_gain_db: Some(g),
                ..Default::default()
            }),
            user_rating: rating.map(|r| ((r / 2.0).round() as u32).clamp(1, 5)),
            play_count: i.user_data.as_ref().and_then(|u| u.play_count),
            added_at: i.date_created.as_deref().and_then(parse_iso_utc),
            album: i.album,
            album_id: i.album_id,
            title: i.name.unwrap_or_default(),
            id: i.id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(flavor: Flavor, base: &str) -> JellyfinClient {
        JellyfinClient::new(
            flavor,
            JellyfinCredentials {
                base_url: base.into(),
                user_id: "u1".into(),
                username: "anna".into(),
                access_token: "TOK".into(),
                device_id: "dev1".into(),
            },
        )
        .unwrap()
    }

    fn parse_items(json: &str) -> Vec<JItem> {
        serde_json::from_str::<ItemsResponse>(json).unwrap().items
    }

    /// Test gegen einen ECHTEN Server: den öffentlichen Jellyfin-Demo-Server
    /// (Zugang "demo" mit leerem Passwort, von Jellyfin selbst veröffentlicht).
    ///
    /// Prüft, was die anderen Tests nicht können: ob unsere Strukturen und
    /// URLs zu einem wirklichen Server passen. Er braucht Internet und ist
    /// deshalb mit `#[ignore]` markiert. Er LIEST nur: Der Demo-Server gehört
    /// allen, Favoriten, Playlists und Wiedergabezähler fassen wir nicht an.
    ///
    ///     cargo test live_jellyfin -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_jellyfin_demo() {
        crate::core::runtime::block_on(async {
            let client = JellyfinClient::login(
                Flavor::Jellyfin,
                "https://demo.jellyfin.org/stable",
                "demo",
                "",
            )
            .await
            .expect("login");
            let creds = client.credentials();
            // Das Token darf nirgends im Klartext ausgegeben werden: nur die Länge.
            println!("Angemeldet als {} (Token {} Zeichen)", creds.username, creds.access_token.len());

            let info = client.ping().await.expect("ping");
            println!("Server: {:?} {:?}", info.server_type, info.server_version);

            let newest = client.newest_albums(5).await.expect("newest");
            assert!(!newest.is_empty(), "keine Alben");
            println!("Neu: {} Alben, erstes: {} ({:?})", newest.len(), newest[0].name, newest[0].artist);

            let recent = client.recent_albums(5).await.expect("recent");
            println!("Zuletzt gehoert: {} Alben", recent.len());

            let page = client.albums_page(0, 2).await.expect("albums_page");
            let page2 = client.albums_page(2, 2).await.expect("albums_page 2");
            assert!(!page.is_empty());
            assert!(page.iter().all(|a| page2.iter().all(|b| a.id != b.id)), "Seiten ueberlappen");
            println!("Alben-Seiten: {} + {}", page.len(), page2.len());

            let album = client.album(&newest[0].id).await.expect("album");
            assert!(!album.tracks.is_empty(), "Album ohne Titel");
            let t = &album.tracks[0];
            println!(
                "Titel: {} / {:?} / {:?} s / Nr {:?} / RG {:?} / Cover {:?}",
                t.title, t.artist, t.duration_secs, t.track_number, t.replay_gain, t.cover_art_id
            );

            let artists = client.artists().await.expect("artists");
            assert!(!artists.is_empty());
            let artist = client.artist(&artists[0].id).await.expect("artist");
            assert!(!artist.albums.is_empty(), "Kuenstler ohne Alben");
            println!(
                "Kuenstler: {} ({} Alben, Zaehler {:?})",
                artist.artist.name,
                artist.albums.len(),
                artists[0].album_count
            );

            let search = client.search("a").await.expect("search");
            println!(
                "Suche 'a': {} Titel, {} Alben, {} Kuenstler",
                search.tracks.len(),
                search.albums.len(),
                search.artists.len()
            );
            assert!(!search.tracks.is_empty() || !search.albums.is_empty());

            let songs = client.songs_page(0, 3).await.expect("songs_page");
            let songs2 = client.songs_page(3, 3).await.expect("songs_page 2");
            assert!(!songs.is_empty());
            assert!(songs.iter().all(|a| songs2.iter().all(|b| a.id != b.id)), "Titel-Seiten ueberlappen");
            println!(
                "Titel-Liste: {} + {}, added_at {:?}, play_count {:?}",
                songs.len(),
                songs2.len(),
                songs[0].added_at,
                songs[0].play_count
            );

            let playlists = client.playlists().await.expect("playlists");
            println!("Playlists: {}", playlists.len());
            if let Some(p) = playlists.first() {
                let detail = client.playlist(&p.id).await.expect("playlist");
                println!("Playlist '{}': {} Titel", detail.playlist.name, detail.tracks.len());
            }

            let starred = client.starred().await.expect("starred");
            println!(
                "Favoriten: {} Titel, {} Alben, {} Kuenstler",
                starred.tracks.len(),
                starred.albums.len(),
                starred.artists.len()
            );

            let lyrics = client.lyrics(&t.id).await.expect("lyrics");
            println!("Songtext vorhanden: {}", lyrics.is_some());

            // Stream und Cover wirklich anfordern (nur Status und Typ; die
            // Antwort wird nicht heruntergeladen).
            let http = reqwest::Client::new();
            for (label, q) in [
                ("Original", StreamQuality { format: AudioFormat::Original, bitrate_kbps: 0 }),
                ("Opus 96", StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 96 }),
                ("AAC 128", StreamQuality { format: AudioFormat::Aac, bitrate_kbps: 128 }),
                ("MP3 128", StreamQuality { format: AudioFormat::Mp3, bitrate_kbps: 128 }),
            ] {
                let resp = http.get(client.stream_url(&t.id, q)).send().await.expect("stream");
                println!("Stream {label}: HTTP {} / {:?}", resp.status(), resp.headers().get("content-type"));
                assert!(resp.status().is_success(), "Stream {label} fehlgeschlagen");
            }

            // Ein Album MIT Bild nehmen: Nicht jedes hat eins, und dann liefert
            // der Server zu Recht 404 (die App zeigt dafür einen Platzhalter).
            let with_cover = newest.iter().chain(page.iter()).find(|a| a.cover_art_id.is_some());
            let cover_id = with_cover.expect("kein Album mit Cover").cover_art_id.clone().unwrap();
            let resp = http.get(client.cover_url(&cover_id, 64)).send().await.expect("cover");
            println!("Cover: HTTP {} / {:?}", resp.status(), resp.headers().get("content-type"));
            assert!(resp.status().is_success());
        });
    }

    #[test]
    fn base_url_is_normalized() {
        use Flavor::*;
        assert_eq!(normalize_base_url("media.local:8096/", Jellyfin), "https://media.local:8096");
        assert_eq!(
            normalize_base_url("http://x/web/index.html#!/home.html", Jellyfin),
            "http://x"
        );
        // Ein Unterpfad gehört bei Jellyfin zur Adresse und bleibt.
        assert_eq!(normalize_base_url("https://x/jellyfin/", Jellyfin), "https://x/jellyfin");
        // `/emby` ist bei Emby nur das API-Präfix, bei Jellyfin ein echter Pfad.
        assert_eq!(normalize_base_url("https://x/emby", Emby), "https://x");
        assert_eq!(normalize_base_url("https://x/emby", Jellyfin), "https://x/emby");
    }

    #[test]
    fn parses_login_response() {
        let json = br#"{"User":{"Name":"demo","ServerId":"s","Id":"4ed1","HasPassword":true},
            "SessionInfo":{"Id":"x"},"AccessToken":"abc123","ServerId":"s"}"#;
        let a = parse_login(Flavor::Jellyfin, StatusCode::OK, json).unwrap();
        assert_eq!(a.access_token, "abc123");
        assert_eq!(a.user.id, "4ed1");
        assert_eq!(a.user.name.as_deref(), Some("demo"));
    }

    #[test]
    fn wrong_password_gives_clear_error() {
        let err = parse_login(Flavor::Jellyfin, StatusCode::UNAUTHORIZED, b"Error processing request.")
            .err()
            .unwrap();
        // library::login erkennt "Passwort" und probiert dann keine zweite Adresse.
        assert!(err.to_string().contains("Passwort falsch"));
        let err = parse_login(Flavor::Emby, StatusCode::NOT_FOUND, b"").err().unwrap();
        assert!(err.to_string().contains("Emby"));
        // Webseite statt Server: verständlicher Fehler, kein Absturz.
        let err = parse_login(Flavor::Jellyfin, StatusCode::OK, b"<html>").err().unwrap();
        assert!(err.to_string().contains("Jellyfin"));
    }

    #[test]
    fn expired_token_asks_for_new_login() {
        let e = status_error(StatusCode::UNAUTHORIZED).unwrap();
        assert!(e.to_string().contains("neu anmelden"));
        assert!(status_error(StatusCode::NO_CONTENT).is_none());
    }

    #[test]
    fn parses_album_with_tracks() {
        let album = parse_items(
            r#"{"Items":[{"Name":"Nemesis","Id":"al1","Type":"MusicAlbum","ProductionYear":2024,
              "RunTimeTicks":1830508539,"ChildCount":2,"AlbumArtist":"Leap Fidei",
              "AlbumArtists":[{"Name":"Leap Fidei","Id":"ar1"}],
              "UserData":{"Rating":10,"PlayCount":0,"IsFavorite":true},
              "ImageTags":{"Primary":"tag"}}],"TotalRecordCount":1}"#,
        );
        let tracks = parse_items(
            r#"{"Items":[{"Name":"Bend","Id":"s1","Type":"Audio","Album":"Nemesis","AlbumId":"al1",
              "AlbumPrimaryImageTag":"x","Artists":["A","B"],"ArtistItems":[{"Name":"A","Id":"ar9"}],
              "RunTimeTicks":2675461220,"IndexNumber":3,"ParentIndexNumber":2,"ProductionYear":2006,
              "Genres":["Electronic"],"DateCreated":"2019-11-28T06:07:13.4661661Z",
              "UserData":{"PlayCount":45,"IsFavorite":false,"Rating":7},
              "ImageTags":{},"NormalizationGain":-6.5}]}"#,
        );
        let a: Album = album.into_iter().next().unwrap().into();
        assert_eq!(a.name, "Nemesis");
        assert_eq!(a.artist.as_deref(), Some("Leap Fidei"));
        assert_eq!(a.artist_id.as_deref(), Some("ar1"));
        assert_eq!(a.song_count, Some(2));
        assert_eq!(a.duration_secs, Some(183));
        assert_eq!(a.cover_art_id.as_deref(), Some("al1"));
        assert!(a.starred);

        let t: Track = tracks.into_iter().next().unwrap().into();
        assert_eq!(t.title, "Bend");
        assert_eq!(t.artist.as_deref(), Some("A, B"));
        assert_eq!(t.artist_id.as_deref(), Some("ar9"));
        assert_eq!(t.album_id.as_deref(), Some("al1"));
        // Titel ohne eigenes Bild nutzt das Albumcover.
        assert_eq!(t.cover_art_id.as_deref(), Some("al1"));
        assert_eq!((t.track_number, t.disc_number, t.year), (Some(3), Some(2), Some(2006)));
        assert_eq!(t.duration_secs, Some(268));
        assert_eq!(t.genre.as_deref(), Some("Electronic"));
        assert_eq!(t.play_count, Some(45));
        // Bewertung 7 von 10 -> 3,5 -> 4 Sterne.
        assert_eq!(t.user_rating, Some(4));
        assert_eq!(t.replay_gain.unwrap().track_gain_db, Some(-6.5));
        assert_eq!(t.added_at, Some(1_574_921_233));
        assert!(!t.starred);
    }

    #[test]
    fn parses_search_result() {
        let artists = parse_items(
            r#"{"Items":[{"Name":"Binaerpilot","Id":"ar1","Type":"MusicArtist",
              "ImageTags":{"Primary":"t"},"AlbumCount":0}]}"#,
        );
        let albums = parse_items(r#"{"Items":[{"Name":"Album","Id":"al1","ImageTags":{}}]}"#);
        let tracks = parse_items(r#"{"Items":[{"Name":"Lied","Id":"s1"},{"Name":"Lied 2","Id":"s2"}]}"#);
        let r = build_search(artists, albums, tracks);
        assert_eq!((r.artists.len(), r.albums.len(), r.tracks.len()), (1, 1, 2));
        assert_eq!(r.artists[0].cover_art_id.as_deref(), Some("ar1"));
        // Zähler 0 gilt als "unbekannt".
        assert_eq!(r.artists[0].album_count, None);
        // Album ohne Bild: kein Cover (statt einer 404-URL).
        assert_eq!(r.albums[0].cover_art_id, None);
        // Leere Antwort ohne "Items" ist kein Fehler.
        assert!(parse_items(r#"{"TotalRecordCount":0}"#).is_empty());
    }

    #[test]
    fn parses_synced_and_plain_lyrics() {
        let synced = br#"{"Metadata":{"IsSynced":true},"Lyrics":[
            {"Start":10000000,"Text":"Zeile 1"},{"Start":25000000,"Text":"Zeile 2"}]}"#;
        let l = parse_lyrics(synced).unwrap();
        assert!(l.synced);
        // 1 Sekunde = 10 Mio. Ticks.
        assert_eq!(l.lines[0].start_ms, Some(1000));
        assert_eq!(l.lines[1].start_ms, Some(2500));
        let plain = br#"{"Lyrics":[{"Text":"Nur Text"}]}"#;
        let l = parse_lyrics(plain).unwrap();
        assert!(!l.synced && l.lines[0].start_ms.is_none());
        assert!(parse_lyrics(br#"{"Lyrics":[]}"#).is_none());
        assert!(parse_lyrics(b"kaputt").is_none());
    }

    #[test]
    fn play_date_formats() {
        // 2024-03-01 12:00:00 UTC (nach einem Schalttag: prüft die Datumsformel).
        assert_eq!(format_play_date(Flavor::Jellyfin, 1_709_294_400), "2024-03-01T12:00:00Z");
        assert_eq!(format_play_date(Flavor::Emby, 1_709_294_400), "20240301120000");
        assert_eq!(format_play_date(Flavor::Jellyfin, 0), "1970-01-01T00:00:00Z");
        // Umkehrprobe mit dem Parser aus subsonic.rs.
        assert_eq!(parse_iso_utc(&format_play_date(Flavor::Jellyfin, 1_574_921_233)), Some(1_574_921_233));
    }

    #[test]
    fn stream_url_requests_quality() {
        let c = client(Flavor::Jellyfin, "https://j.example/base");
        let orig = c.stream_url("42", StreamQuality { format: AudioFormat::Original, bitrate_kbps: 0 });
        assert!(orig.starts_with("https://j.example/base/Audio/42/stream?"));
        assert!(orig.contains("static=true") && orig.contains("ApiKey=TOK") && orig.contains("api_key=TOK"));
        assert!(!orig.contains("audioCodec"));

        let opus = c.stream_url("42", StreamQuality { format: AudioFormat::Opus, bitrate_kbps: 96 });
        assert!(opus.starts_with("https://j.example/base/Audio/42/stream.opus?"));
        // Bitrate in Bit/s, nicht in kbit/s.
        assert!(opus.contains("audioCodec=opus") && opus.contains("audioBitRate=96000"));
        assert!(!opus.contains("static"));
        let mp3 = c.stream_url("42", StreamQuality { format: AudioFormat::Mp3, bitrate_kbps: 128 });
        assert!(mp3.contains("stream.mp3?") && mp3.contains("audioBitRate=128000"));
    }

    #[test]
    fn emby_uses_prefix_and_its_own_token_names() {
        let c = client(Flavor::Emby, "https://e.example");
        let cover = c.cover_url("al1", 300);
        assert!(cover.starts_with("https://e.example/emby/Items/al1/Images/Primary?"));
        assert!(cover.contains("maxWidth=300") && cover.contains("maxHeight=300"));
        assert!(cover.contains("api_key=TOK") && cover.contains("X-Emby-Token=TOK"));
        assert!(!cover.contains("ApiKey="));
        let s = c.stream_url("7", StreamQuality { format: AudioFormat::Aac, bitrate_kbps: 160 });
        assert!(s.starts_with("https://e.example/emby/Audio/7/stream.aac?"));
        assert!(s.contains("audioBitRate=160000"));
    }

    #[test]
    fn ids_cannot_break_out_of_the_path() {
        let c = client(Flavor::Jellyfin, "https://j.example");
        let u = c.cover_url("../../Users", 64);
        assert!(!u.contains("/../"), "{u}");
    }
}
