//! Start der App.

use anyhow::Result;

use crate::core::storage;

/// Wird von flutter_rust_bridge automatisch bei `RustLib.init()` aufgerufen.
#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // Leitet Rust-Panics und Logs sinnvoll an Dart weiter.
    flutter_rust_bridge::setup_default_user_utils();
    // Nur Warnungen und Fehler protokollieren.
    //
    // WARUM? Die JNI-Bibliothek schreibt auf der feinsten Stufe mehrere
    // Zeilen pro Aufruf. Der Audio-Thread ruft Rust aber viele Male pro
    // Sekunde auf – das wären tausende Protokollzeilen pro Minute, die
    // Rechenzeit und Akku kosten und das Systemprotokoll fluten.
    log::set_max_level(log::LevelFilter::Warn);
}

/// Merkt sich, ob der Kern schon eingerichtet ist.
static CORE_READY: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// Richtet den Kern genau EINMAL ein.
///
/// Es gibt zwei mögliche Aufrufer, und keiner weiß, wer zuerst kommt:
///  - Dart beim Start der Oberfläche (`init_core`)
///  - der Android-Wiedergabedienst über JNI (z. B. Start über Android Auto,
///    ganz ohne Oberfläche)
/// Der Mutex sorgt dafür, dass Datenbank und Einstellungen nicht doppelt
/// geöffnet werden, selbst wenn beide gleichzeitig kommen.
pub(crate) fn ensure_core(data_dir: String, storage_key: Vec<u8>) -> Result<()> {
    let mut ready = CORE_READY.lock().unwrap_or_else(|e| e.into_inner());
    if *ready {
        return Ok(());
    }
    storage::set_data_dir(data_dir.into())?;
    // Schlüssel aus dem Android-Keystore: Damit liegt die Anmeldung
    // verschlüsselt auf der Platte.
    storage::set_storage_key(&storage_key);
    crate::core::db::open(&storage::data_dir()?)?;
    // Einstellungen: Ein Fehler hier (unlesbare Datei) darf den Start nicht
    // verhindern; dann gelten eben die Standardwerte.
    let _ = crate::api::settings::load_from_disk();
    // Klang-Einstellungen laden und sofort für den Audio-Thread aktivieren.
    let _ = crate::api::dsp::load_from_disk();
    // Gespeicherte Anmeldung laden, damit man nicht bei jedem Start neu
    // einloggen muss. Fehler hier sind nicht fatal: dann eben Login-Seite.
    let _ = crate::api::library::restore_session();
    // Letzte Warteschlange laden: So kann Android nach einem Neustart
    // "Weiterspielen" anbieten, und die App zeigt den zuletzt gehörten Titel.
    crate::api::queue::restore_from_disk();
    log::set_max_level(log::LevelFilter::Warn);
    *ready = true;
    Ok(())
}

/// Richtet den Kern ein. Muss einmal nach `RustLib.init()` aufgerufen werden.
///
/// `data_dir` kommt aus Dart (`getApplicationSupportDirectory()`), weil nur
/// die Android-Seite weiß, wo die App ihre privaten Dateien ablegen darf.
/// `storage_key` ist der 32-Byte-Schlüssel aus dem Android-Keystore (leer
/// auf Plattformen ohne Schlüsselspeicher).
pub fn init_core(data_dir: String, storage_key: Vec<u8>) -> Result<()> {
    ensure_core(data_dir, storage_key)
}
