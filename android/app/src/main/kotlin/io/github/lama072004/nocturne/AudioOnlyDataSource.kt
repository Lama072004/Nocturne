package io.github.lama072004.nocturne

import android.net.Uri
import androidx.annotation.OptIn
import androidx.media3.common.PlaybackException
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DataSpec
import androidx.media3.datasource.HttpDataSource
import androidx.media3.datasource.TransferListener
import java.io.IOException

/**
 * Hülle um eine HTTP-Datenquelle, die nur AUDIO durchlässt.
 *
 * WARUM nötig? Subsonic-Server melden Fehler (falsches Passwort, Titel
 * gelöscht, Umwandlung fehlgeschlagen) oft mit HTTP 200 und einer kurzen
 * JSON-/XML-Nachricht statt mit einem Fehler-Statuscode. Ohne diese Prüfung
 * würde Media3 diese Nachricht
 *  - als "Audio" in den Streaming-Cache schreiben (der Titel bliebe kaputt,
 *    bis der Cache ihn verdrängt), und
 *  - als FERTIGEN Download ablegen (der Nutzer sähe "heruntergeladen",
 *    offline kommt aber nichts).
 *
 * Die Prüfung passiert in [open], also BEVOR ein einziges Byte gelesen oder
 * in einen Cache geschrieben wird. Fehlt der Content-Type ganz, lassen wir
 * die Antwort durch: Manche Server schicken bei Rohdateien keinen mit.
 *
 * Die Ausnahme ist eine [HttpDataSource.HttpDataSourceException] mit dem
 * Fehlercode ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE. Wichtig: KEIN
 * Netzwerk-Fehlercode – sonst würde der Dienst nach "Netz wieder da"
 * endlos neu versuchen (siehe PlaybackService.waitForNetwork).
 */
@OptIn(UnstableApi::class)
class AudioOnlyDataSource(private val upstream: HttpDataSource) : DataSource {

    override fun addTransferListener(transferListener: TransferListener) =
        upstream.addTransferListener(transferListener)

    override fun open(dataSpec: DataSpec): Long {
        val length = upstream.open(dataSpec)
        val type = contentType()
        if (type != null && type in ERROR_TYPES) {
            try {
                upstream.close()
            } catch (_: IOException) {
                // Egal: Wir brechen ohnehin ab.
            }
            throw HttpDataSource.HttpDataSourceException(
                "Server lieferte keine Audiodaten (Content-Type: $type)",
                dataSpec,
                PlaybackException.ERROR_CODE_IO_INVALID_HTTP_CONTENT_TYPE,
                HttpDataSource.HttpDataSourceException.TYPE_OPEN,
            )
        }
        return length
    }

    /** "application/json; charset=utf-8" -> "application/json"; `null` wenn keiner. */
    private fun contentType(): String? = upstream.responseHeaders.entries
        // Groß-/Kleinschreibung von Header-Namen ist bei HTTP beliebig.
        .firstOrNull { it.key.equals("Content-Type", ignoreCase = true) }
        ?.value?.firstOrNull()
        ?.substringBefore(';')?.trim()?.lowercase()
        ?.takeIf { it.isNotEmpty() }

    override fun read(buffer: ByteArray, offset: Int, length: Int): Int =
        upstream.read(buffer, offset, length)

    override fun getUri(): Uri? = upstream.uri

    override fun getResponseHeaders(): Map<String, List<String>> = upstream.responseHeaders

    override fun close() = upstream.close()

    /** Baut für jede Anfrage eine geprüfte Quelle um eine frische HTTP-Quelle. */
    class Factory(private val upstream: HttpDataSource.Factory) : DataSource.Factory {
        override fun createDataSource(): DataSource = AudioOnlyDataSource(upstream.createDataSource())
    }

    companion object {
        /** Content-Types, mit denen Subsonic-Server Fehlermeldungen schicken. */
        private val ERROR_TYPES = setOf(
            "application/json",
            "text/xml",
            "application/xml",
            "text/html",
        )
    }
}
