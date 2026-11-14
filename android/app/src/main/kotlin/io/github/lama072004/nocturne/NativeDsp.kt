package io.github.lama072004.nocturne

import java.nio.ByteBuffer

/**
 * Kotlin-Seite der direkten Verbindung zum Rust-Kern (siehe
 * `rust/src/jni_bridge.rs`).
 *
 * Die `external`-Funktionen sind in Rust geschrieben. Android findet sie über
 * ihren Namen `Java_io_github_lama072004_nocturne_NativeDsp_<Methode>`.
 * Wird dieses Paket oder diese Klasse umbenannt, müssen die Namen in Rust
 * mit angepasst werden – sonst stürzt der erste Aufruf ab.
 */
object NativeDsp {

    /**
     * `false`, wenn die Rust-Bibliothek nicht geladen werden konnte. Dann
     * läuft die Musik einfach ohne Effekte, statt dass der Dienst abstürzt.
     */
    val available: Boolean = try {
        // Dieselbe .so-Datei, die auch Flutter lädt. Android lädt sie nur
        // einmal pro Prozess: Beide Seiten teilen sich dadurch dieselben
        // Einstellungen im Speicher.
        System.loadLibrary("rust_lib_musicplayer")
        true
    } catch (e: UnsatisfiedLinkError) {
        false
    }

    /**
     * Zwischengespeichert, ob gerade irgendein Effekt eingeschaltet ist.
     * Der Audio-Thread liest nur dieses Feld und spart sich bei
     * ausgeschalteten Effekten den Sprung nach Rust komplett.
     */
    @Volatile
    var active: Boolean = false

    /** Fragt Rust und aktualisiert [active]. */
    fun refreshActive(): Boolean {
        active = available && nativeIsActive()
        return active
    }

    // Das Einrichten des Kerns (früher `nativeInit` hier) liegt jetzt in
    // NativeCore: Es braucht zusätzlich den Schlüssel aus SecretStore und hat
    // mit dem Klang nichts zu tun.

    /** Verarbeitet 16-Bit-PCM im direkten Puffer an Ort und Stelle. */
    @JvmStatic external fun nativeProcess(
        buffer: ByteBuffer,
        offset: Int,
        length: Int,
        sampleRate: Int,
        channels: Int,
    )

    @JvmStatic external fun nativeReset()

    @JvmStatic external fun nativeIsActive(): Boolean

    /** ReplayGain-Werte des beginnenden Titels; `Float.NaN` = fehlt. */
    @JvmStatic external fun nativeSetTrackGain(
        trackGain: Float,
        albumGain: Float,
        trackPeak: Float,
        albumPeak: Float,
    )

    @JvmStatic external fun nativeSetOutputDevice(key: String, name: String)
}
