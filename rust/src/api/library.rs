//! Anmeldung und Bibliothek (Alben, Suche, Cover, Stream-URLs).

use std::path::PathBuf;
use std::sync::{Arc, LazyLock, RwLock};

use anyhow::{anyhow, Result};

use crate::api::models::{
    Album, AlbumDetail, Artist, ArtistDetail, Lyrics, Playlist, PlaylistDetail, SearchResult,
    ServerInfo, ServerKind, StarKind, Starred,
};
use crate::api::settings::{self, StreamQuality};
use crate::core::{runtime, storage};
use crate::server::{jellyfin, plex, subsonic, SavedSession, ServerClient};

/// Die aktive Server-Verbindung. `Arc`, damit eine laufende Anfrage den
/// Client behalten darf, auch wenn sich der Nutzer währenddessen abmeldet.
static CLIENT: LazyLock<RwLock<Option<Arc<ServerClient>>>> = LazyLock::new(|| RwLock::new(None));

pub(crate) fn client() -> Result<Arc<ServerClient>> {
    CLIENT.read().unwrap_or_else(|e| e.into_inner()).clone().ok_or_else(|| anyhow!("Nicht angemeldet"))
}

fn session_path() -> Result<PathBuf> {
    // Frühere Versionen legten die Anmeldung unverschlüsselt in
    // `session.json` ab. Diese Datei wird hier entfernt, damit kein Token
    // im Klartext auf dem Gerät liegen bleibt. (Einmalig neu anmelden.)
    let dir = storage::data_dir()?;
    let _ = storage::remove(&dir.join("session.json"));
    let _ = storage::remove(&dir.join("session.bad"));
    Ok(dir.join("session.enc"))
}

/// Meldet sich an einem Server an und merkt sich die Verbindung.
///
/// Erst wenn der `ping` erfolgreich war, wird gespeichert. So landet nie
/// eine kaputte Anmeldung auf der Platte.
///
/// Je nach Servertyp bedeuten die Felder etwas Leicht anderes:
/// * Subsonic, Jellyfin, Emby: Benutzername und Passwort des Servers.
/// * Plex: Benutzername und Passwort des plex.tv-Kontos. Bleibt der
///   Benutzername leer, gilt das Passwort-Feld als fertiges Plex-Token.
pub fn login(kind: ServerKind, url: String, username: String, password: String) -> Result<ServerInfo> {
    // Plex meldet bei plex.tv an, nicht am Server: Das Token holen wir einmal
    // vorab, damit die Probe von HTTPS und HTTP unten nicht doppelt anmeldet.
    let plex_login = if kind == ServerKind::Plex {
        Some(plex_token(&username, &password)?)
    } else {
        None
    };

    // Hat der Nutzer kein "http://" oder "https://" getippt, probieren wir
    // erst HTTPS (sicher) und dann HTTP (üblich im Heimnetz). Mit
    // ausdrücklicher Angabe wird genau diese verwendet.
    let typed = url.trim();
    let candidates: Vec<String> = if typed.starts_with("http://") || typed.starts_with("https://") {
        vec![typed.to_owned()]
    } else {
        vec![format!("https://{typed}"), format!("http://{typed}")]
    };

    let mut last_error = anyhow!("Keine Server-Adresse angegeben");
    for candidate in candidates {
        match runtime::block_on_net(connect(kind, &candidate, &username, &password, plex_login.as_ref())) {
            Ok((client, info)) => {
                storage::write_secret_json(&session_path()?, &client.saved())?;
                *CLIENT.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(client));
                return Ok(info);
            }
            Err(e) => {
                // Falsche Zugangsdaten heißen: Der Server WURDE erreicht.
                // Dann bringt die zweite Adresse nichts, und die klare
                // Meldung soll nicht von einem Verbindungsfehler überdeckt
                // werden. Die Wörter stammen aus den Fehlermeldungen der
                // Adapter ("Passwort falsch", "Plex-Token ungültig", …).
                let msg = e.to_string();
                let wrong_login =
                    msg.contains("Passwort") || msg.contains("ungültig") || msg.contains("Zwei-Faktor");
                last_error = e;
                if wrong_login {
                    break;
                }
            }
        }
    }
    Err(last_error)
}

/// Plex: (Token, Client-ID). Aus Benutzername und Passwort bei plex.tv, oder
/// das Passwort-Feld selbst als Token, wenn kein Benutzername angegeben ist.
fn plex_token(username: &str, password: &str) -> Result<(String, String)> {
    let client_id = plex::new_client_id();
    if username.trim().is_empty() {
        let token = password.trim();
        if token.is_empty() {
            return Err(anyhow!("Bitte Benutzername und Passwort oder ein Plex-Token angeben"));
        }
        return Ok((token.to_owned(), client_id));
    }
    let token = runtime::block_on_net(plex::sign_in(username.trim(), &password, &client_id))?;
    Ok((token, client_id))
}

/// Baut den passenden Client für eine Adresse und prüft ihn mit einem `ping`.
async fn connect(
    kind: ServerKind,
    base_url: &str,
    username: &str,
    password: &str,
    plex_login: Option<&(String, String)>,
) -> Result<(ServerClient, ServerInfo)> {
    let client = match kind {
        ServerKind::Subsonic => ServerClient::Subsonic(subsonic::SubsonicClient::new(
            subsonic::SubsonicCredentials::from_password(base_url, username, password),
        )?),
        ServerKind::Jellyfin => ServerClient::Jellyfin(
            jellyfin::JellyfinClient::login(jellyfin::Flavor::Jellyfin, base_url, username, password).await?,
        ),
        ServerKind::Emby => ServerClient::Emby(
            jellyfin::JellyfinClient::login(jellyfin::Flavor::Emby, base_url, username, password).await?,
        ),
        ServerKind::Plex => {
            let (token, client_id) = plex_login.ok_or_else(|| anyhow!("Plex-Anmeldung fehlt"))?;
            ServerClient::Plex(plex::PlexClient::connect(base_url, token, client_id).await?)
        }
    };
    let info = client.ping().await?;
    Ok((client, info))
}

/// Lädt die gespeicherte Anmeldung. Absichtlich OHNE Netzwerkzugriff: Die
/// App soll auch offline sofort starten können.
pub(crate) fn restore_session() -> Result<bool> {
    let Some(saved): Option<SavedSession> = storage::read_secret_json(&session_path()?)? else {
        return Ok(false);
    };
    *CLIENT.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(ServerClient::from_saved(saved)?));
    Ok(true)
}

#[flutter_rust_bridge::frb(sync)]
pub fn is_logged_in() -> bool {
    CLIENT.read().unwrap_or_else(|e| e.into_inner()).is_some()
}

pub fn logout() -> Result<()> {
    *CLIENT.write().unwrap_or_else(|e| e.into_inner()) = None;
    storage::remove(&session_path()?)
}

pub fn newest_albums(count: u32) -> Result<Vec<Album>> {
    let c = client()?;
    runtime::block_on_net(c.newest_albums(count))
}

pub fn recent_albums(count: u32) -> Result<Vec<Album>> {
    let c = client()?;
    runtime::block_on_net(c.recent_albums(count))
}

pub fn album_detail(id: String) -> Result<AlbumDetail> {
    let c = client()?;
    runtime::block_on_net(c.album(&id))
}

/// Alle Alben alphabetisch, seitenweise. Die UI lädt die nächste Seite
/// erst, wenn man ans Ende scrollt. So werden bei großen Sammlungen nicht
/// tausende Alben auf einmal übertragen.
pub fn albums_page(offset: u32, size: u32) -> Result<Vec<Album>> {
    let c = client()?;
    runtime::block_on_net(c.albums_page(offset, size))
}

pub fn artists() -> Result<Vec<Artist>> {
    let c = client()?;
    runtime::block_on_net(c.artists())
}

pub fn artist_detail(id: String) -> Result<ArtistDetail> {
    let c = client()?;
    runtime::block_on_net(c.artist(&id))
}

pub fn playlists() -> Result<Vec<Playlist>> {
    let c = client()?;
    runtime::block_on_net(c.playlists())
}

pub fn playlist_detail(id: String) -> Result<PlaylistDetail> {
    let c = client()?;
    runtime::block_on_net(c.playlist(&id))
}

pub fn create_playlist(name: String, song_ids: Vec<String>) -> Result<()> {
    let c = client()?;
    runtime::block_on_net(c.create_playlist(&name, &song_ids))
}

pub fn add_to_playlist(playlist_id: String, song_ids: Vec<String>) -> Result<()> {
    let c = client()?;
    runtime::block_on_net(c.add_to_playlist(&playlist_id, &song_ids))
}

pub fn starred() -> Result<Starred> {
    let c = client()?;
    runtime::block_on_net(c.starred())
}

pub fn set_starred(kind: StarKind, id: String, starred: bool) -> Result<()> {
    let c = client()?;
    runtime::block_on_net(c.set_starred(kind, &id, starred))
}

/// Songtext zu einem Titel. `None`, wenn der Server keinen hat.
pub fn lyrics(song_id: String) -> Result<Option<Lyrics>> {
    let c = client()?;
    runtime::block_on_net(c.lyrics(&song_id))
}

pub fn search(query: String) -> Result<SearchResult> {
    let c = client()?;
    runtime::block_on_net(c.search(&query))
}

/// Cover-URL. `sync`, weil nur ein String zusammengebaut wird – die UI
/// braucht ihn sofort beim Zeichnen einer Zeile.
#[flutter_rust_bridge::frb(sync)]
pub fn cover_url(cover_id: String, size: u32) -> Option<String> {
    CLIENT.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|c| c.cover_url(&cover_id, size))
}

/// Netzwerkart, die Dart erkennt (connectivity_plus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkKind {
    Wifi,
    Mobile,
    /// Kein Netz: Nur Downloads sind abspielbar.
    None,
}

static NETWORK: RwLock<NetworkKind> = RwLock::new(NetworkKind::Wifi);

/// Dart meldet hier jeden Netzwechsel. Davon hängt ab, welche Qualität für
/// NEUE Stream-URLs verwendet wird.
#[flutter_rust_bridge::frb(sync)]
pub fn set_network_kind(kind: NetworkKind) {
    *NETWORK.write().unwrap_or_else(|e| e.into_inner()) = kind;
}

// Nur über JNI (Android-Dienst) erreichbar; auf dem PC sonst "unbenutzt".
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn current_stream_quality() -> StreamQuality {
    let s = settings::current();
    match *NETWORK.read().unwrap_or_else(|e| e.into_inner()) {
        NetworkKind::Mobile => s.quality_mobile,
        // Bei "kein Netz" spielt ohnehin die lokale Datei; die URL ist dann egal.
        NetworkKind::Wifi | NetworkKind::None => s.quality_wifi,
    }
}

/// URL zum HERUNTERLADEN eines Titels (nutzt die Download-Qualität aus den
/// Einstellungen, unabhängig vom aktuellen Netz).
#[flutter_rust_bridge::frb(sync)]
pub fn download_url(song_id: String) -> Option<String> {
    let q = settings::current().quality_download;
    CLIENT.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|c| c.stream_url(&song_id, q))
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn stream_url(song_id: &str) -> Option<String> {
    let q = current_stream_quality();
    CLIENT.read().unwrap_or_else(|e| e.into_inner()).as_ref().map(|c| c.stream_url(song_id, q))
}
