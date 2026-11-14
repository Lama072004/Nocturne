//! Klangbearbeitung.
//!
//! Geplanter Einbau: Ein eigener `AudioProcessor` in ExoPlayer (Kotlin) gibt
//! jeden PCM-Block per JNI an diese Module weiter. ExoPlayer kümmert sich
//! weiter um Dekodieren, Puffern und lückenlose Übergänge, Rust nur um den
//! Klang.
//!
//! Regeln für Code in diesem Modul (er läuft im Echtzeit-Audio-Thread):
//!  - KEINE Speicherreservierung während `process` (kein Vec::push, kein Box::new).
//!    Allokationen können blockieren und hörbare Knackser verursachen.
//!  - KEINE Sperren (Mutex) im Verarbeitungspfad.
//!  - `f32` reicht für die Samples. Die Filter-Zustände rechnen wir trotzdem
//!    in `f64`, damit tiefe Bässe nicht durch Rundungsfehler rauschen.

pub mod biquad;
pub mod chain;
pub mod compressor;
pub mod replaygain;
