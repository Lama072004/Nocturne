//! Direkte Verbindung Kotlin -> Rust (JNI), am Flutter-Teil vorbei.
//!
//! WARUM ein zweiter Weg neben flutter_rust_bridge?
//! Der Android-Dienst muss den Rust-Kern auch dann erreichen, wenn die
//! Flutter-Oberfläche gar nicht läuft (Bildschirm aus, App aus den letzten
//! Apps gewischt, Start über Android Auto):
//!  - der Audio-Thread von ExoPlayer für die Klangeffekte (`NativeDsp`),
//!  - der Wiedergabe-Dienst für Stream- und Cover-Adressen (`NativeCore`).
//!
//! Beide Wege landen in derselben geladenen Bibliothek und damit bei
//! denselben globalen Daten: Was die Oberfläche einstellt, gilt sofort auch
//! für den Dienst.
//!
//! Die Funktionsnamen folgen dem JNI-Schema `Java_<Paket>_<Klasse>_<Methode>`
//! und gehören zu den Kotlin-Klassen `NativeDsp` und `NativeCore` im Paket
//! `io.github.lama072004.nocturne`. Wer das Paket umbenennt, muss die Namen
//! hier anpassen.
//!
//! # Panics
//! Ein Rust-Panic, der über die JNI-Grenze hinausläuft, beendet den ganzen
//! App-Prozess. Deshalb fängt [`guard`] jeden Panic ab und liefert einen
//! harmlosen Ersatzwert: Im schlimmsten Fall läuft ein Audio-Block ohne
//! Effekte durch, statt dass die App abstürzt.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{LazyLock, Mutex};

use jni::objects::{JByteArray, JByteBuffer, JClass, JString};
use jni::sys::{jboolean, jfloat, jint, jlong, jstring, JNI_FALSE, JNI_TRUE};
use jni::JNIEnv;

use crate::api::models::ReplayGainInfo;
use crate::dsp::chain::{self, DspChain};

/// Die eine Effektkette des Players. Der Mutex ist praktisch nie umkämpft:
/// Nur der Audio-Thread benutzt sie.
static CHAIN: LazyLock<Mutex<DspChain>> = LazyLock::new(|| Mutex::new(DspChain::default()));

/// Führt `f` aus und fängt einen Panic ab (siehe Modul-Kommentar).
fn guard<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

fn string(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(Into::into).unwrap_or_default()
}

/// Gibt einen Rust-String als Java-String zurück; `None` wird zu `null`.
fn java_string(env: &mut JNIEnv, s: Option<String>) -> jstring {
    s.and_then(|s| env.new_string(s).ok())
        .map(|s| s.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

// ---------------------------------------------------------------------------
// NativeCore: Einrichtung und Adressen für den Wiedergabe-Dienst
// ---------------------------------------------------------------------------

/// Richtet den Kern ein, falls Flutter das noch nicht getan hat.
/// `storage_key` ist der Schlüssel aus dem Android-Keystore, mit dem die
/// Anmeldung verschlüsselt gespeichert wird.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeInit(
    mut env: JNIEnv,
    _class: JClass,
    data_dir: JString,
    storage_key: JByteArray,
) {
    guard((), || {
        let dir = string(&mut env, &data_dir);
        let key = env.convert_byte_array(&storage_key).unwrap_or_default();
        // Fehler hier dürfen den Dienst nicht abstürzen lassen: Dann läuft
        // er eben ohne gespeicherte Anmeldung.
        let _ = crate::api::app::ensure_core(dir, key);
    })
}

/// Abspiel-Adresse eines Titels in der gerade passenden Qualität.
///
/// Der Dienst löst die Adresse selbst auf, statt sie von der Oberfläche im
/// MediaItem zu bekommen: Sie enthält die Anmeldedaten und soll deshalb
/// nicht über die Mediensitzung für andere Apps lesbar sein.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeStreamUrl(
    mut env: JNIEnv,
    _class: JClass,
    song_id: JString,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let id = string(&mut env, &song_id);
        let url = crate::api::library::stream_url(&id);
        java_string(&mut env, url)
    })
}

/// Adresse eines Covers in der gewünschten Kantenlänge.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeCoverUrl(
    mut env: JNIEnv,
    _class: JClass,
    cover_id: JString,
    size: jint,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let id = string(&mut env, &cover_id);
        let url = crate::api::library::cover_url(id, size.clamp(32, 2048) as u32);
        java_string(&mut env, url)
    })
}

/// Kinder eines Knotens im Inhaltsbaum als JSON-Liste (Android Auto,
/// Assistent, Widgets). Bei einem Fehler eine leere Liste – der Dienst
/// zeigt dann einfach nichts an, statt abzustürzen.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeBrowse(
    mut env: JNIEnv,
    _class: JClass,
    parent_id: JString,
    german: jboolean,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let parent = string(&mut env, &parent_id);
        let items = crate::core::browse::children(&parent, german != JNI_FALSE).unwrap_or_default();
        let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_owned());
        java_string(&mut env, Some(json))
    })
}

/// Sprachsuche: abspielbare Titel als JSON-Liste.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeSearch(
    mut env: JNIEnv,
    _class: JClass,
    query: JString,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let query = string(&mut env, &query);
        let items = crate::core::browse::search(&query).unwrap_or_default();
        let json = serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_owned());
        java_string(&mut env, Some(json))
    })
}

/// Spielt einen Eintrag aus dem Inhaltsbaum: setzt die Warteschlange und
/// liefert sie als JSON (`null`, wenn der Eintrag nicht abspielbar ist).
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativePlayFromMediaId(
    mut env: JNIEnv,
    _class: JClass,
    media_id: JString,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let id = string(&mut env, &media_id);
        let json = crate::api::queue::play_from_media_id(&id);
        java_string(&mut env, json)
    })
}

/// Zuletzt gespeicherte Warteschlange samt Position ("Weiterspielen").
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeResume(
    mut env: JNIEnv,
    _class: JClass,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let json = crate::api::queue::resume_json();
        java_string(&mut env, json)
    })
}

#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeSavePosition(
    _env: JNIEnv,
    _class: JClass,
    position_ms: jlong,
) {
    guard((), || crate::api::queue::save_position(position_ms))
}

/// Netzart vom Dienst: 0 = kein Netz, 1 = WLAN/ungemessen, 2 = mobil.
/// Davon hängt ab, in welcher Qualität neue Titel angefordert werden –
/// auch wenn die Flutter-Oberfläche nicht läuft.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeSetNetwork(
    _env: JNIEnv,
    _class: JClass,
    kind: jint,
) {
    use crate::api::library::NetworkKind;
    guard((), || {
        crate::api::library::set_network_kind(match kind {
            0 => NetworkKind::None,
            2 => NetworkKind::Mobile,
            _ => NetworkKind::Wifi,
        })
    })
}

/// Ein Titel wurde gehört: in den Verlauf schreiben und dem Server melden.
/// Der Dienst zählt die Hörzeit selbst, weil die Flutter-Oberfläche
/// beendet sein kann, während die Musik weiterläuft.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeReportPlay(
    mut env: JNIEnv,
    _class: JClass,
    entry_id: JString,
    listened_ms: jlong,
) {
    guard((), || {
        let id = string(&mut env, &entry_id);
        if let Some(track) = crate::api::queue::track_for_entry(&id) {
            let secs = (listened_ms.max(0) / 1000) as u32;
            let _ = crate::api::stats::report_play(track, secs);
        }
    })
}

/// "Läuft gerade" an den Server melden.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeNowPlaying(
    mut env: JNIEnv,
    _class: JClass,
    entry_id: JString,
) {
    guard((), || {
        let id = string(&mut env, &entry_id);
        if let Some(track) = crate::api::queue::note_playing(&id) {
            crate::api::stats::report_now_playing(track.id);
        }
    })
}

/// Titelwechsel ohne laufende Oberfläche: Rust führt die Warteschlange nach.
/// Liefert die neue Liste als JSON, falls ExoPlayer sie braucht, sonst null.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeCore_nativeTransition(
    mut env: JNIEnv,
    _class: JClass,
    entry_id: JString,
    wrapped: jboolean,
) -> jstring {
    guard(std::ptr::null_mut(), || {
        let id = string(&mut env, &entry_id);
        let json = crate::api::queue::service_transition(&id, wrapped != 0);
        java_string(&mut env, json)
    })
}

// ---------------------------------------------------------------------------
// NativeDsp: Klangeffekte im Audio-Thread
// ---------------------------------------------------------------------------

/// Verarbeitet `length` Bytes 16-Bit-PCM im direkten Puffer an Ort und Stelle.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeDsp_nativeProcess(
    env: JNIEnv,
    _class: JClass,
    buffer: JByteBuffer,
    offset: jint,
    length: jint,
    sample_rate: jint,
    channels: jint,
) {
    guard((), || {
        let (Ok(ptr), Ok(capacity)) =
            (env.get_direct_buffer_address(&buffer), env.get_direct_buffer_capacity(&buffer))
        else {
            return;
        };
        let (offset, length) = (offset.max(0) as usize, length.max(0) as usize);
        // Grenzen prüfen, bevor wir auf rohen Speicher zugreifen. `checked_add`
        // schützt zusätzlich vor einem Überlauf der Summe.
        let Some(end) = offset.checked_add(length) else { return };
        if ptr.is_null() || end > capacity {
            return;
        }
        // SAFETY: `ptr` zeigt auf einen direkten ByteBuffer mit mindestens
        // `capacity` Bytes; der Bereich [offset, offset+length) wurde gerade
        // geprüft. Kotlin fasst den Puffer während dieses Aufrufs nicht an.
        let data = unsafe { std::slice::from_raw_parts_mut(ptr.add(offset), length) };
        // Ein vergiftetes Schloss (früherer Panic) übernehmen wir einfach:
        // Der Zustand der Kette ist nur Filter-Verlauf, der sich selbst heilt.
        let mut chain = CHAIN.lock().unwrap_or_else(|e| e.into_inner());
        chain.process_i16_bytes(data, sample_rate.max(1) as u32, channels.max(0) as usize);
    })
}

/// Filterverläufe löschen (nach dem Spulen oder Titel-Sprung).
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeDsp_nativeReset(
    _env: JNIEnv,
    _class: JClass,
) {
    guard((), || {
        CHAIN.lock().unwrap_or_else(|e| e.into_inner()).reset();
    })
}

#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeDsp_nativeIsActive(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    guard(JNI_FALSE, || if chain::is_active() { JNI_TRUE } else { JNI_FALSE })
}

/// ReplayGain-Werte des Titels, der jetzt beginnt. NaN = Wert fehlt.
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeDsp_nativeSetTrackGain(
    _env: JNIEnv,
    _class: JClass,
    track_gain: jfloat,
    album_gain: jfloat,
    track_peak: jfloat,
    album_peak: jfloat,
) {
    guard((), || {
        let opt = |v: f32| (!v.is_nan()).then_some(v);
        chain::set_track_gain(Some(&ReplayGainInfo {
            track_gain_db: opt(track_gain),
            album_gain_db: opt(album_gain),
            track_peak: opt(track_peak),
            album_peak: opt(album_peak),
        }));
    })
}

/// Android meldet das aktuelle Ausgabegerät (für EQ-Profile je Gerät).
#[no_mangle]
pub extern "system" fn Java_io_github_lama072004_nocturne_NativeDsp_nativeSetOutputDevice(
    mut env: JNIEnv,
    _class: JClass,
    key: JString,
    name: JString,
) {
    guard((), || {
        let (key, name) = (string(&mut env, &key), string(&mut env, &name));
        crate::api::dsp::set_output_device(key, name);
    })
}
