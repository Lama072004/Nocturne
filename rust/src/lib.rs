//! Rust-Kern der Musik-App.
//!
//! Aufteilung:
//!  - `api`    : alles, was Dart sehen darf (flutter_rust_bridge erzeugt daraus
//!               automatisch Dart-Code in `lib/src/rust/`)
//!  - `core`   : interne Logik (Warteschlange, Speicher, Laufzeit)
//!  - `server` : Adapter für die verschiedenen Server-APIs
//!  - `dsp`    : Klangbearbeitung (EQ, ReplayGain …), später per JNI im
//!               Audio-Pfad von ExoPlayer
//!
//! Die Trennung sorgt dafür, dass `core`, `server` und `dsp` ganz normal mit
//! `cargo test` getestet werden können – ohne Handy, ohne Flutter.

pub mod api;
pub mod core;
pub mod dsp;
pub mod server;
mod frb_generated;
// Nur auf Android: direkte JNI-Verbindung für den Audio-Thread.
#[cfg(target_os = "android")]
mod jni_bridge;
