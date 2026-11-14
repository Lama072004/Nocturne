//! Öffentliche Schnittstelle für Dart.
//!
//! Faustregel für alle Funktionen hier:
//!  - `#[frb(sync)]` nur für Dinge, die sofort aus dem RAM kommen (z. B.
//!    Einstellungen lesen). Die blockieren die UI nicht spürbar.
//!  - Alles mit Netzwerk oder Datei ist eine normale Funktion -> läuft auf
//!    einem Hintergrund-Thread, Dart bekommt ein `Future`.

pub mod app;
pub mod downloads;
pub mod dsp;
pub mod library;
pub mod local;
pub mod models;
pub mod prefs;
pub mod queue;
pub mod session;
pub mod settings;
pub mod smart;
pub mod stats;
