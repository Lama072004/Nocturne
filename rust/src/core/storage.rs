//! Zugriff auf das App-Datenverzeichnis und sicheres Speichern kleiner Dateien.
//!
//! Rust kennt den Android-Pfad der App nicht von selbst. Dart (oder der
//! Android-Dienst) übergibt ihn beim Start an `ensure_core`. Ab dann kennt
//! der Kern sein Verzeichnis für Einstellungen, Datenbank und Anmeldung.
//!
//! Drei Dinge macht dieses Modul robust (alle drei stammen aus einem
//! Code-Review):
//!  1. **Schreiben ist absturzsicher**: erst in eine temporäre Datei, auf die
//!     Platte zwingen (`sync_all`), dann umbenennen. Ohne `sync_all` kann nach
//!     einem Stromausfall eine leere Datei unter dem richtigen Namen liegen.
//!  2. **Lesen verzeiht kaputte Dateien**: Lässt sich eine Datei nicht
//!     lesen, wird sie zur Seite gelegt (`.bad`) und der Standardwert
//!     benutzt. Sonst würde die App nach einem einzigen Schreibfehler nie
//!     wieder starten.
//!  3. **Anmeldedaten liegen verschlüsselt** auf der Platte. Den Schlüssel
//!     liefert Android aus seinem Schlüsselspeicher (Keystore); er liegt nie
//!     im Klartext im App-Ordner.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{anyhow, bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Schlüssel für [`write_secret_json`]. Fehlt er (z. B. in Tests auf dem
/// PC), wird unverschlüsselt gespeichert.
static STORAGE_KEY: OnceLock<[u8; 32]> = OnceLock::new();

/// Sorgt dafür, dass nie zwei Schreibvorgänge gleichzeitig dieselbe
/// temporäre Datei benutzen (z. B. zwei schnelle Regler-Änderungen im
/// Equalizer). Schreibvorgänge sind selten und kurz, ein gemeinsames Schloss
/// für alle Dateien reicht.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn set_data_dir(dir: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&dir).with_context(|| format!("Kann {dir:?} nicht anlegen"))?;
    // `set` schlägt fehl, wenn schon gesetzt (z. B. Hot-Restart in Flutter).
    // Das ist harmlos, der Pfad ändert sich ja nicht.
    let _ = DATA_DIR.set(dir);
    Ok(())
}

pub fn data_dir() -> Result<PathBuf> {
    DATA_DIR.get().cloned().ok_or_else(|| anyhow!("Der Kern ist noch nicht eingerichtet"))
}

/// Setzt den Schlüssel für verschlüsselte Dateien. Andere Längen als
/// 32 Byte werden ignoriert (dann bleibt es bei Klartext).
pub fn set_storage_key(key: &[u8]) {
    if let Ok(k) = <[u8; 32]>::try_from(key) {
        let _ = STORAGE_KEY.set(k);
    }
}

/// Legt eine unlesbare Datei zur Seite, damit sie den Start nicht blockiert,
/// aber für eine Fehlersuche erhalten bleibt.
fn quarantine(path: &Path) {
    let _ = std::fs::rename(path, path.with_extension("bad"));
}

/// Liest eine JSON-Datei. `Ok(None)`, wenn sie nicht existiert ODER nicht
/// lesbar ist (dann gilt der Standardwert des Aufrufers).
pub fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            quarantine(path);
            Ok(None)
        }
    }
}

/// Schreibt Bytes absturzsicher: temporäre Datei, auf die Platte zwingen,
/// umbenennen. Umbenennen ist ein einziger Schritt – danach liegt entweder
/// die alte oder die neue Datei vollständig vor, nie eine halbe.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    // Vergiftetes Schloss (ein anderer Thread ist beim Schreiben
    // abgestürzt) einfach übernehmen: Die Daten schützt das Umbenennen.
    let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

/// Wie [`write_json`], aber verschlüsselt (AES-256-GCM).
///
/// Dateiformat: 12 Byte Zufallswert (Nonce) + verschlüsselte Daten. Der
/// Nonce ist bei jedem Schreiben neu; GCM erkennt außerdem jede
/// Veränderung der Datei.
pub fn write_secret_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let plain = serde_json::to_vec(value)?;
    let Some(key) = STORAGE_KEY.get() else {
        // Kein Schlüssel vorhanden (nur außerhalb von Android): Klartext.
        return write_atomic(path, &plain);
    };
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("Ungültiger Schlüssel"))?;
    let nonce_bytes: [u8; 12] = rand::random();
    let encrypted = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), plain.as_ref())
        .map_err(|_| anyhow!("Verschlüsseln fehlgeschlagen"))?;
    let mut out = Vec::with_capacity(12 + encrypted.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&encrypted);
    write_atomic(path, &out)
}

/// Gegenstück zu [`write_secret_json`]. Lässt sich die Datei nicht
/// entschlüsseln (anderer Schlüssel nach Neuinstallation, beschädigt), gilt
/// sie als nicht vorhanden: Der Nutzer muss sich dann neu anmelden, die App
/// startet aber normal.
pub fn read_secret_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let decrypt = || -> Result<T> {
        let Some(key) = STORAGE_KEY.get() else {
            return Ok(serde_json::from_slice(&bytes)?);
        };
        if bytes.len() < 12 {
            bail!("Datei zu kurz");
        }
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| anyhow!("Ungültiger Schlüssel"))?;
        let plain = cipher
            .decrypt(Nonce::from_slice(&bytes[..12]), &bytes[12..])
            .map_err(|_| anyhow!("Entschlüsseln fehlgeschlagen"))?;
        Ok(serde_json::from_slice(&plain)?)
    };
    match decrypt() {
        Ok(v) => Ok(Some(v)),
        Err(_) => {
            quarantine(path);
            Ok(None)
        }
    }
}

pub fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("nocturne_storage_tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn broken_file_falls_back_to_default_and_is_set_aside() {
        let path = temp("broken.json");
        std::fs::write(&path, b"{ das ist kein json").unwrap();
        let read: Option<Vec<u32>> = read_json(&path).unwrap();
        assert_eq!(read, None);
        // Die kaputte Datei liegt jetzt unter .bad und blockiert nichts mehr.
        assert!(!path.exists());
        assert!(path.with_extension("bad").exists());
    }

    #[test]
    fn json_round_trip() {
        let path = temp("ok.json");
        write_json(&path, &vec![1u32, 2, 3]).unwrap();
        assert_eq!(read_json::<Vec<u32>>(&path).unwrap(), Some(vec![1, 2, 3]));
    }

    #[test]
    fn secret_round_trip_and_wrong_data() {
        // Der Schlüssel ist global; dieser Test ist der einzige, der ihn setzt.
        set_storage_key(&[7u8; 32]);
        let path = temp("secret.enc");
        write_secret_json(&path, &"geheim".to_owned()).unwrap();
        // Auf der Platte steht das Wort nicht im Klartext.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.windows(6).any(|w| w == b"geheim"));
        assert_eq!(read_secret_json::<String>(&path).unwrap().as_deref(), Some("geheim"));

        // Verfälschte Datei -> gilt als nicht vorhanden statt als Absturz.
        std::fs::write(&path, b"zu kurz").unwrap();
        assert_eq!(read_secret_json::<String>(&path).unwrap(), None);
    }
}
