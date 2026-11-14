//! Kleine Merkzettel der Oberfläche: Ansicht (Raster/Liste) und Sortierung
//! je Bibliotheks-Filter, zuletzt gesuchte Begriffe, angeheftete Einträge.
//!
//! WARUM nicht in `AppSettings`?
//! Das sind keine Einstellungen, die der Nutzer bewusst trifft, sondern
//! Zustände, die die Oberfläche sich merkt. Sie ändern sich oft und es
//! kommen laufend neue dazu. Mit einer einfachen Schlüssel/Wert-Ablage muss
//! dafür nicht jedes Mal die Schnittstelle zwischen Rust und Dart erweitert
//! werden. Gespeichert wird in der Tabelle `meta` der Datenbank.

use anyhow::Result;

use crate::core::db;

/// Präfix, damit Merkzettel nicht mit internen Einträgen (z. B. Zeitpunkt
/// des letzten Abgleichs) kollidieren.
const PREFIX: &str = "pref:";

pub fn get_pref(key: String) -> Result<Option<String>> {
    db::get_meta(&format!("{PREFIX}{key}"))
}

/// Setzt einen Merkzettel; `None` löscht ihn.
pub fn set_pref(key: String, value: Option<String>) -> Result<()> {
    let key = format!("{PREFIX}{key}");
    match value {
        Some(v) => db::set_meta(&key, &v),
        None => db::delete_meta(&key),
    }
}
