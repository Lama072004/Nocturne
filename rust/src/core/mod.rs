//! Interne Logik ohne direkten Dart-Zugriff.
//!
//! Alles hier ist für flutter_rust_bridge unsichtbar. Die öffentliche
//! Schnittstelle zur UI liegt in `crate::api` und ruft diese Module auf.
//! So bleibt die Logik testbar, ohne dass man die Brücke braucht.

pub mod browse;
pub mod db;
pub mod queue;
pub mod runtime;
pub mod smart;
pub mod storage;
