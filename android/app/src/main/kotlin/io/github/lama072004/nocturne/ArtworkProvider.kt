package io.github.lama072004.nocturne

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.util.Log
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.MessageDigest
import java.util.concurrent.ConcurrentHashMap

/**
 * Liefert Cover-Bilder unter `content://<applicationId>.artwork/<coverId>`.
 *
 * WARUM ein eigener Provider statt der Cover-URL des Servers?
 * Die Cover-URL eines Subsonic-Servers enthält Token und Salt. Steht sie als
 * `artworkUri` im MediaItem, kann jede App, die sich mit der MediaSession
 * verbindet (oder die Metadaten der Benachrichtigung liest), sie auslesen und
 * damit im Namen des Nutzers auf den Server zugreifen. Die content://-Adresse
 * enthält dagegen nur die Cover-ID; die echte URL baut Rust erst hier, im
 * eigenen Prozess.
 *
 * WARUM exported="true" ohne Berechtigung – ist das unkritisch?
 * Android Auto, Wear OS und die System-Oberfläche laden das Cover SELBST
 * über diese Adresse. Sie bekommen keine URI-Freigabe von uns, also muss der
 * Provider offen sein. Das ist vertretbar, weil er
 *  - nur lesend ist (insert/update/delete werfen, openFile nur "r"),
 *  - ausschließlich Bilddaten herausgibt, nie Adressen, Token oder Salt,
 *  - nur Cover zu IDs liefert, die der Aufrufer schon kennen muss.
 * Restrisiko: Eine fremde App könnte Cover-IDs durchprobieren und so sehen,
 * welche Cover auf dem Server des Nutzers liegen. Das halten wir für
 * hinnehmbar; Anmeldedaten verlassen die App nicht.
 *
 * AKKU/DATEN: Jedes Cover wird nur EINMAL geladen und dann aus
 * `cacheDir/artwork` geliefert. Android darf diesen Ordner bei
 * Speichermangel selbst leeren – dann wird bei Bedarf neu geladen.
 */
class ArtworkProvider : ContentProvider() {

    override fun onCreate(): Boolean = true

    /**
     * Läuft auf einem Binder-Thread (bzw. dem Thread des Aufrufers im
     * eigenen Prozess). Blockierendes Laden ist hier erlaubt und üblich.
     */
    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw FileNotFoundException("Nur lesender Zugriff")
        val coverId = uri.pathSegments.singleOrNull()
            ?.takeIf { it.isNotBlank() && it.length <= MAX_ID_LENGTH }
            ?: throw FileNotFoundException("Ungültige Cover-Adresse")
        val ctx = context ?: throw FileNotFoundException("Kein Kontext")
        val file = coverFile(ctx, coverId)
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    }

    /**
     * Wird von `openTypedAssetFile(…, "image/…")` geprüft (so laden z. B.
     * Systemteile Bilder). Liegt das Cover schon im Cache, erkennen wir den
     * echten Typ an den ersten Bytes; sonst ist JPEG die beste Schätzung.
     * Bildladeroutinen erkennen das Format ohnehin selbst.
     */
    override fun getType(uri: Uri): String {
        val ctx = context ?: return "image/jpeg"
        val id = uri.pathSegments.singleOrNull() ?: return "image/jpeg"
        return sniffType(File(artworkDir(ctx), cacheName(id)))
    }

    // Nur lesend: Alles andere ist nicht vorgesehen.
    override fun query(
        uri: Uri,
        projection: Array<out String>?,
        selection: String?,
        selectionArgs: Array<out String>?,
        sortOrder: String?,
    ): Cursor? = null

    override fun insert(uri: Uri, values: ContentValues?): Uri? =
        throw UnsupportedOperationException("Nur lesend")

    override fun update(
        uri: Uri,
        values: ContentValues?,
        selection: String?,
        selectionArgs: Array<out String>?,
    ): Int = throw UnsupportedOperationException("Nur lesend")

    override fun delete(uri: Uri, selection: String?, selectionArgs: Array<out String>?): Int =
        throw UnsupportedOperationException("Nur lesend")

    companion object {
        private const val TAG = "ArtworkProvider"
        private const val MAX_ID_LENGTH = 256
        private const val COVER_SIZE = 512
        /** Schutz gegen einen Server, der statt eines Bildes Unmengen schickt. */
        private const val MAX_BYTES = 10L * 1024 * 1024

        /** Eine Sperre je Cover: Fragen Benachrichtigung und Android Auto
         *  gleichzeitig, wird trotzdem nur einmal geladen. */
        private val locks = ConcurrentHashMap<String, Any>()

        /** Baut die content://-Adresse für ein Cover (genutzt von PlayerBridge). */
        fun uriFor(context: Context, coverId: String): Uri = Uri.Builder()
            .scheme("content")
            .authority(context.packageName + ".artwork")
            .appendPath(coverId)
            .build()

        /**
         * Für die Homescreen-Widgets: Cover-Datei aus dem Cache, bei Bedarf
         * einmal vom Server geladen. BLOCKIERT (Netz) -> nur im Hintergrund.
         * Die Widgets brauchen ein Bitmap (RemoteViews können keine
         * content://-Adresse eines eigenen Providers zuverlässig laden).
         *
         * @return `null`, wenn das Cover nicht zu bekommen ist.
         */
        fun coverFileBlocking(context: Context, coverId: String): File? {
            if (coverId.isBlank() || coverId.length > MAX_ID_LENGTH) return null
            return try {
                coverFile(context.applicationContext, coverId)
            } catch (e: IOException) {
                // FileNotFoundException gehört dazu: nicht angemeldet, kein Bild …
                null
            }
        }

        private fun artworkDir(context: Context) = File(context.cacheDir, "artwork")

        /**
         * Dateiname = SHA-1 der Cover-ID. Damit kann eine fremde Cover-ID
         * (z. B. "../../files/…") nie aus dem Ordner herausführen.
         */
        private fun cacheName(coverId: String): String =
            MessageDigest.getInstance("SHA-1")
                .digest(coverId.toByteArray(Charsets.UTF_8))
                .joinToString("") { "%02x".format(it) }

        private fun coverFile(context: Context, coverId: String): File {
            val dir = artworkDir(context).apply { mkdirs() }
            val name = cacheName(coverId)
            val file = File(dir, name)
            if (file.length() > 0) return file
            val lock = locks.computeIfAbsent(name) { Any() }
            try {
                synchronized(lock) {
                    // Hat ein anderer Thread es inzwischen geladen?
                    if (file.length() > 0) return file
                    download(context, coverId, dir, file)
                    return file
                }
            } finally {
                locks.remove(name, lock)
            }
        }

        private fun download(context: Context, coverId: String, dir: File, target: File) {
            // Der Provider kann der erste Teil der App sein, der in einem
            // frischen Prozess läuft (fremde App fragt ein Cover ab).
            NativeCore.ensureInit(context)
            val url = NativeCore.coverUrl(coverId, COVER_SIZE)
                ?: throw FileNotFoundException("Nicht angemeldet")
            val tmp = File(dir, target.name + ".part")
            var conn: HttpURLConnection? = null
            try {
                conn = (URL(url).openConnection() as HttpURLConnection).apply {
                    connectTimeout = 10_000
                    readTimeout = 15_000
                    instanceFollowRedirects = true
                    setRequestProperty("User-Agent", "Nocturne")
                }
                if (conn.responseCode !in 200..299) {
                    throw FileNotFoundException("HTTP ${conn.responseCode}")
                }
                // Auch hier: Subsonic schickt Fehler als JSON/XML mit HTTP 200.
                val type = conn.contentType?.substringBefore(';')?.trim()?.lowercase()
                if (type != null && !type.startsWith("image/") && type != "application/octet-stream") {
                    throw FileNotFoundException("Kein Bild (Content-Type: $type)")
                }
                var total = 0L
                conn.inputStream.use { input ->
                    tmp.outputStream().use { out ->
                        val buf = ByteArray(16 * 1024)
                        while (true) {
                            val n = input.read(buf)
                            if (n < 0) break
                            total += n
                            if (total > MAX_BYTES) throw FileNotFoundException("Cover zu groß")
                            out.write(buf, 0, n)
                        }
                    }
                }
                if (total == 0L) throw FileNotFoundException("Leeres Cover")
                // Erst in einen Zwischennamen schreiben, dann umbenennen: Ein
                // halb geladenes Bild darf nie als "fertig" gelten.
                if (!tmp.renameTo(target)) throw FileNotFoundException("Cache nicht beschreibbar")
            } catch (e: FileNotFoundException) {
                tmp.delete()
                throw e
            } catch (e: IOException) {
                tmp.delete()
                Log.w(TAG, "Cover konnte nicht geladen werden", e)
                throw FileNotFoundException("Cover konnte nicht geladen werden: ${e.message}")
            } finally {
                conn?.disconnect()
            }
        }

        private fun sniffType(file: File): String {
            val head = ByteArray(12)
            val n = try {
                if (!file.exists()) return "image/jpeg"
                file.inputStream().use { it.read(head) }
            } catch (e: IOException) {
                return "image/jpeg"
            }
            fun at(i: Int) = head[i].toInt() and 0xFF
            return when {
                n >= 4 && at(0) == 0x89 && at(1) == 0x50 && at(2) == 0x4E && at(3) == 0x47 -> "image/png"
                n >= 12 && String(head, 0, 4, Charsets.US_ASCII) == "RIFF" &&
                    String(head, 8, 4, Charsets.US_ASCII) == "WEBP" -> "image/webp"
                n >= 3 && String(head, 0, 3, Charsets.US_ASCII) == "GIF" -> "image/gif"
                else -> "image/jpeg"
            }
        }
    }
}
