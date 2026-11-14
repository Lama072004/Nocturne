//! Angaben zur aktuellen Anmeldung, für die Profilseite.
//!
//! Bewusst OHNE Token oder Passwort: Die Oberfläche bekommt nur, was sie
//! anzeigen soll (Benutzer, Server-Adresse, Servertyp).

use crate::api::library;
use crate::api::models::ServerKind;
use crate::server::SavedSession;

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub username: String,
    /// Adresse des Servers, wie sie beim Anmelden ermittelt wurde.
    pub server_url: String,
    pub kind: ServerKind,
}

/// `None`, wenn niemand angemeldet ist.
#[flutter_rust_bridge::frb(sync)]
pub fn session_info() -> Option<SessionInfo> {
    let client = library::client().ok()?;
    // Der `match` ist absichtlich vollständig: Kommt ein neuer Servertyp
    // dazu, meldet der Compiler, dass hier eine Zeile fehlt.
    Some(match client.saved() {
        SavedSession::Subsonic(c) => SessionInfo {
            username: c.username,
            server_url: c.base_url,
            kind: ServerKind::Subsonic,
        },
        SavedSession::Jellyfin(c) => SessionInfo {
            username: c.username,
            server_url: c.base_url,
            kind: ServerKind::Jellyfin,
        },
        SavedSession::Emby(c) => SessionInfo {
            username: c.username,
            server_url: c.base_url,
            kind: ServerKind::Emby,
        },
        // Bei Plex kennt die App nur das Token, keinen Benutzernamen.
        SavedSession::Plex(c) => SessionInfo {
            username: String::new(),
            server_url: c.base_url,
            kind: ServerKind::Plex,
        },
    })
}
