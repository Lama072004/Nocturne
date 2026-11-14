package io.github.lama072004.nocturne

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import java.io.IOException
import java.security.KeyStore
import java.security.SecureRandom
import java.security.UnrecoverableKeyException
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Liefert den Schlüssel, mit dem der Rust-Kern seine Anmeldedaten auf der
 * Platte verschlüsselt.
 *
 * WARUM nicht direkt einen Schlüssel im AndroidKeyStore für Rust?
 * Schlüssel im KeyStore verlassen die Hardware nie – Rust könnte damit nur
 * über viele JNI-Aufrufe arbeiten. Stattdessen gibt es zwei Stufen:
 *
 *   1. Ein zufälliger 32-Byte-Schlüssel ("Datenschlüssel") für Rust.
 *   2. Ein AES-GCM-Schlüssel im AndroidKeyStore ("Hüllschlüssel"), mit dem
 *      der Datenschlüssel verschlüsselt in SharedPreferences liegt.
 *
 * Der Datenschlüssel steht dadurch NIE im Klartext auf der Platte. Wer nur
 * die App-Dateien kopiert (Backup, gerootetes Gerät ohne laufende App),
 * kann damit nichts anfangen, weil der Hüllschlüssel am Gerät hängt.
 *
 * Kaputter Eintrag: Lässt sich der gespeicherte Datenschlüssel nicht mehr
 * entschlüsseln (z. B. Prefs per Auto-Backup auf ein neues Handy
 * übertragen, KeyStore-Eintrag dort aber nicht), wird ein neuer erzeugt.
 * Folge: Rust kann die alten Anmeldedaten nicht lesen, der Nutzer meldet
 * sich einmal neu an. Das ist gewollt – lieber neu anmelden als Klartext.
 *
 * Vorübergehende Fehler des KeyStores (z. B. Dienst hängt) werfen dagegen
 * eine Ausnahme, statt einen neuen Schlüssel zu erzeugen. Sonst würde ein
 * kurzer Schluckauf die gespeicherte Anmeldung vernichten.
 */
object SecretStore {
    private const val TAG = "SecretStore"
    private const val PREFS = "secret_store"
    private const val PREF_WRAPPED = "storage_key_v1"
    private const val KEY_ALIAS = "nocturne_storage_wrap_v1"
    private const val KEYSTORE = "AndroidKeyStore"
    private const val TRANSFORMATION = "AES/GCM/NoPadding"
    private const val KEY_BYTES = 32
    private const val GCM_TAG_BITS = 128

    /** Im Speicher zwischengespeichert: Der KeyStore ist vergleichsweise langsam. */
    private var cached: ByteArray? = null

    /** Gespeicherter Eintrag ist unbrauchbar und muss ersetzt werden. */
    private class CorruptEntry(cause: Throwable?) : Exception(cause)

    /**
     * Stabiler 32-Byte-Schlüssel für Rust. Thread-sicher (synchronisiert),
     * darf aber wegen des KeyStores nicht in einer engen Schleife laufen.
     * Gibt jedes Mal eine KOPIE zurück, damit niemand den Puffer verändert.
     */
    @Synchronized
    fun key(context: Context): ByteArray {
        cached?.let { return it.copyOf() }
        val prefs = context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val keyStore = KeyStore.getInstance(KEYSTORE).apply { load(null) }

        prefs.getString(PREF_WRAPPED, null)?.let { stored ->
            try {
                val raw = unwrap(keyStore, stored)
                cached = raw
                return raw.copyOf()
            } catch (e: CorruptEntry) {
                Log.w(TAG, "Gespeicherter Schlüssel unlesbar, erzeuge neuen", e.cause)
                // Den Hüllschlüssel gleich mit ersetzen: Er ist evtl. die Ursache
                // (z. B. dauerhaft ungültig geworden).
                keyStore.deleteEntry(KEY_ALIAS)
            }
        }

        val fresh = ByteArray(KEY_BYTES).also { SecureRandom().nextBytes(it) }
        val wrapped = wrap(wrappingKey(keyStore), fresh)
        // commit() statt apply(): Erst wenn der Eintrag sicher auf der Platte
        // liegt, darf Rust mit dem Schlüssel Daten verschlüsseln.
        if (!prefs.edit().putString(PREF_WRAPPED, wrapped).commit()) {
            throw IOException("Schlüssel konnte nicht gespeichert werden")
        }
        cached = fresh
        return fresh.copyOf()
    }

    /** Holt den Hüllschlüssel aus dem KeyStore oder legt ihn an. */
    private fun wrappingKey(keyStore: KeyStore): SecretKey {
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                // Keine Nutzer-Authentifizierung: Der Dienst muss den Schlüssel
                // auch bei gesperrtem Bildschirm lesen können (Android Auto,
                // Wiedergabe nach Neustart).
                .build(),
        )
        return generator.generateKey()
    }

    /** Format: [1 Byte IV-Länge][IV][Chiffretext inkl. GCM-Tag], Base64. */
    private fun wrap(key: SecretKey, raw: ByteArray): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        // Den IV erzeugt der KeyStore selbst (zufällig); vorgeben ist verboten.
        cipher.init(Cipher.ENCRYPT_MODE, key)
        val iv = cipher.iv
        val sealed = cipher.doFinal(raw)
        val out = ByteArray(1 + iv.size + sealed.size)
        out[0] = iv.size.toByte()
        System.arraycopy(iv, 0, out, 1, iv.size)
        System.arraycopy(sealed, 0, out, 1 + iv.size, sealed.size)
        return Base64.encodeToString(out, Base64.NO_WRAP)
    }

    /**
     * Entschlüsselt den gespeicherten Eintrag. Alles, was auf einen kaputten
     * oder nicht mehr passenden Eintrag hindeutet, wird zu [CorruptEntry];
     * andere Fehler (KeyStore vorübergehend weg) fliegen unverändert durch.
     */
    private fun unwrap(keyStore: KeyStore, stored: String): ByteArray {
        try {
            val data = Base64.decode(stored, Base64.NO_WRAP)
            val ivLen = data.firstOrNull()?.toInt() ?: throw CorruptEntry(null)
            if (ivLen !in 12..16 || data.size <= 1 + ivLen) throw CorruptEntry(null)
            val key = keyStore.getKey(KEY_ALIAS, null) as? SecretKey
                ?: throw CorruptEntry(null) // KeyStore-Eintrag fehlt (z. B. nach Backup-Wiederherstellung)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, key, GCMParameterSpec(GCM_TAG_BITS, data, 1, ivLen))
            val raw = cipher.doFinal(data, 1 + ivLen, data.size - 1 - ivLen)
            if (raw.size != KEY_BYTES) throw CorruptEntry(null)
            return raw
        } catch (e: IllegalArgumentException) {
            throw CorruptEntry(e) // kein gültiges Base64
        } catch (e: AEADBadTagException) {
            throw CorruptEntry(e) // falscher Schlüssel oder veränderte Daten
        } catch (e: KeyPermanentlyInvalidatedException) {
            throw CorruptEntry(e)
        } catch (e: UnrecoverableKeyException) {
            throw CorruptEntry(e)
        }
    }
}
