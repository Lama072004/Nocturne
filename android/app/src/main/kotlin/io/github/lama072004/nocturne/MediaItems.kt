package io.github.lama072004.nocturne

import android.content.Context
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * EINE Stelle, an der aus Daten von Dart oder Rust ein Media3-`MediaItem` wird.
 *
 * WARUM zentral? Dieselben Warteschlangen-Einträge kommen auf zwei Wegen an:
 *  - von der Flutter-Oberfläche als Map über den MethodChannel (PlayerBridge),
 *  - direkt aus Rust als JSON, wenn KEINE Oberfläche läuft (PlaybackService:
 *    Android Auto, Assistent, Widget, Weiterspielen nach Neustart).
 * Beide müssen exakt gleich aussehen (gleiche Extras, gleiche Cover-Adresse),
 * sonst verhielte sich ein Titel je nach Startweg anders – z. B. fehlte
 * ReplayGain oder der Dienst fände die Song-ID nicht. Das JSON wird deshalb
 * erst in eine Map übersetzt und dann durch DIESELBE Funktion
 * [fromQueueEntry] geschickt.
 *
 * Außerdem baut diese Datei die Einträge des Inhaltsbaums ([parseBrowseJson])
 * für Android Auto und Assistenten.
 */
object MediaItems {
    private const val TAG = "MediaItems"

    /** Eine fertige Warteschlange mit Startpunkt (aus Rust-JSON). */
    data class Queue(val items: List<MediaItem>, val index: Int, val positionMs: Long)

    // -----------------------------------------------------------------------
    // Warteschlangen-Einträge
    // -----------------------------------------------------------------------

    /** Liste aus Dart (siehe PlayerBridge "setQueue"/"updateQueue"). */
    fun fromDartList(context: Context, raw: List<Map<String, Any?>>?): List<MediaItem> =
        raw.orEmpty().mapNotNull { fromQueueEntry(context, it) }

    /**
     * Ein Warteschlangen-Eintrag -> MediaItem.
     *
     * Erwartete Felder (Dart-Map und Rust-JSON identisch):
     * entryId, songId, title, artist, album, coverId,
     * trackGain, albumGain, trackPeak, albumPeak.
     *
     * @return `null` ohne entryId (der Eintrag wäre nicht wiederzufinden).
     */
    fun fromQueueEntry(context: Context, m: Map<String, Any?>): MediaItem? {
        // Die Media-ID ist eine "Queue-Eintrags-ID" aus Rust, nicht die
        // Song-ID: Derselbe Song kann mehrfach in der Warteschlange stehen.
        val entryId = (m["entryId"] as? String)?.takeIf { it.isNotEmpty() } ?: return null
        val meta = MediaMetadata.Builder()
            .setTitle(m["title"] as? String)
            .setArtist(m["artist"] as? String)
            .setAlbumTitle(m["album"] as? String)
            // SICHERHEIT: Nicht die Cover-URL des Servers (enthält Token
            // und Salt), sondern eine content://-Adresse unseres
            // ArtworkProvider. Fremde Apps sehen nur die Cover-ID.
            .setArtworkUri(coverUri(context, m["coverId"] as? String))
            .setIsPlayable(true)
            .setIsBrowsable(false)
            .setMediaType(MediaMetadata.MEDIA_TYPE_MUSIC)
            .build()
        return MediaItem.Builder()
            .setMediaId(entryId)
            // SICHERHEIT: Keine Stream-URL (ein evtl. mitgeschicktes Feld
            // "url" wird bewusst ignoriert). Alles in einem MediaItem kann über
            // die Session bei anderen Apps landen – auch
            // requestMetadata.mediaUri (Media3 reicht es an ältere
            // Controller weiter). Es reist nur die Song-ID mit; der Dienst
            // baut die URL selbst über Rust (PlaybackService.resolve).
            .setRequestMetadata(
                MediaItem.RequestMetadata.Builder()
                    .setExtras(Bundle().apply {
                        // Der Schlüssel ist IMMER gesetzt (notfalls mit null):
                        // Daran erkennt der Dienst Einträge aus der eigenen
                        // App im Unterschied zu fremden Controllern, die nur
                        // eine Media-ID schicken (siehe PlaybackService.isOwnItem).
                        putString(PlaybackService.EXTRA_SONG_ID, m["songId"] as? String)
                        // ReplayGain-Werte nur eintragen, wenn vorhanden.
                        // Fehlt ein Schlüssel, liest der Dienst NaN = "unbekannt".
                        // Zahl ODER Text annehmen: Rust könnte Werte als
                        // String schicken, Dart schickt immer double.
                        fun gain(key: String, extra: String) {
                            val v = m[key]
                            val f = (v as? Number)?.toFloat() ?: (v as? String)?.toFloatOrNull()
                            f?.takeIf { !it.isNaN() }?.let { putFloat(extra, it) }
                        }
                        gain("trackGain", PlaybackService.EXTRA_TRACK_GAIN)
                        gain("albumGain", PlaybackService.EXTRA_ALBUM_GAIN)
                        gain("trackPeak", PlaybackService.EXTRA_TRACK_PEAK)
                        gain("albumPeak", PlaybackService.EXTRA_ALBUM_PEAK)
                    })
                    .build(),
            )
            .setMediaMetadata(meta)
            .build()
    }

    /**
     * Antwort von `nativePlayFromMediaId` / `nativeResume`:
     * `{"index":N,"positionMs":M,"items":[ITEM,…]}`.
     *
     * @return `null` bei kaputtem JSON oder leerer Liste.
     */
    fun parseQueueJson(context: Context, json: String?): Queue? {
        if (json.isNullOrBlank()) return null
        return try {
            val obj = JSONObject(json)
            val arr = obj.optJSONArray("items") ?: return null
            val items = ArrayList<MediaItem>(arr.length())
            // Index mitzählen: Fällt ein Eintrag weg (ohne entryId), muss der
            // Startindex mitwandern, sonst startete ein falscher Titel.
            val wanted = obj.optInt("index", 0)
            var index = 0
            for (i in 0 until arr.length()) {
                val entry = arr.optJSONObject(i)?.let { fromQueueEntry(context, it.toMap()) } ?: continue
                if (i < wanted) index++
                items += entry
            }
            if (items.isEmpty()) return null
            Queue(items, index.coerceIn(0, items.lastIndex), obj.optLong("positionMs", 0L).coerceAtLeast(0L))
        } catch (e: JSONException) {
            Log.w(TAG, "Warteschlange aus Rust nicht lesbar", e)
            null
        }
    }

    // -----------------------------------------------------------------------
    // Inhaltsbaum (Android Auto, Assistent, Schnellzugriff-Widget)
    // -----------------------------------------------------------------------

    /** Ein Eintrag des Inhaltsbaums, so wie Rust ihn liefert. */
    data class BrowseEntry(
        val id: String,
        val title: String,
        val subtitle: String?,
        val coverId: String?,
        val browsable: Boolean,
        val playable: Boolean,
    )

    /** JSON-Array aus `nativeBrowse`/`nativeSearch` -> Einträge; kaputtes JSON = leer. */
    fun parseBrowseEntries(json: String?): List<BrowseEntry> {
        if (json.isNullOrBlank()) return emptyList()
        return try {
            val arr = JSONArray(json)
            (0 until arr.length()).mapNotNull { i ->
                val o = arr.optJSONObject(i) ?: return@mapNotNull null
                val id = o.optStringOrNull("id") ?: return@mapNotNull null
                BrowseEntry(
                    id = id,
                    title = o.optStringOrNull("title") ?: "",
                    subtitle = o.optStringOrNull("subtitle"),
                    coverId = o.optStringOrNull("coverId"),
                    browsable = o.optBoolean("browsable", false),
                    playable = o.optBoolean("playable", false),
                )
            }
        } catch (e: JSONException) {
            Log.w(TAG, "Inhaltsbaum aus Rust nicht lesbar", e)
            emptyList()
        }
    }

    /** JSON-Array aus `nativeBrowse`/`nativeSearch` -> MediaItems für die Session. */
    fun parseBrowseJson(context: Context, json: String?): List<MediaItem> =
        parseBrowseEntries(json).map { toBrowseItem(context, it) }

    /**
     * Ordner, Album, Playlist oder Titel als MediaItem.
     *
     * Der Medientyp hilft Android Auto bei der Darstellung (Raster für
     * Alben, Liste für Titel) und Assistenten bei der Auswahl. Er wird am
     * Präfix der ID erkannt, den Rust vergibt.
     */
    fun toBrowseItem(context: Context, e: BrowseEntry): MediaItem {
        val type = when {
            !e.browsable -> MediaMetadata.MEDIA_TYPE_MUSIC
            e.id.startsWith("album:") -> MediaMetadata.MEDIA_TYPE_ALBUM
            e.id.startsWith("playlist:") || e.id.startsWith("smart:") ->
                MediaMetadata.MEDIA_TYPE_PLAYLIST
            e.id == "albums" -> MediaMetadata.MEDIA_TYPE_FOLDER_ALBUMS
            e.id == "playlists" -> MediaMetadata.MEDIA_TYPE_FOLDER_PLAYLISTS
            else -> MediaMetadata.MEDIA_TYPE_FOLDER_MIXED
        }
        val meta = MediaMetadata.Builder()
            .setTitle(e.title)
            .setDisplayTitle(e.title)
            .setSubtitle(e.subtitle)
            // Bei Titeln ist der Untertitel der Künstler; so zeigt auch
            // ein Controller, der nur "artist" liest, etwas Sinnvolles.
            .setArtist(if (type == MediaMetadata.MEDIA_TYPE_MUSIC) e.subtitle else null)
            .setArtworkUri(coverUri(context, e.coverId))
            .setIsBrowsable(e.browsable)
            .setIsPlayable(e.playable)
            .setMediaType(type)
            .build()
        // KEINE songId in den Extras: Diese Items sind "fremde" Einträge.
        // Wird eines davon abgespielt, baut der Dienst die Warteschlange über
        // nativePlayFromMediaId (siehe PlaybackService.playForeign).
        return MediaItem.Builder()
            .setMediaId(e.id)
            .setMediaMetadata(meta)
            .build()
    }

    // -----------------------------------------------------------------------
    // Hilfen
    // -----------------------------------------------------------------------

    private fun coverUri(context: Context, coverId: String?): Uri? =
        coverId?.takeIf { it.isNotEmpty() }?.let { ArtworkProvider.uriFor(context, it) }

    /**
     * Cover-ID eines MediaItems zurückgewinnen (für die Widgets). Sie steckt
     * als letzter Pfadteil in der content://-Adresse des ArtworkProvider.
     */
    fun coverIdOf(context: Context, item: MediaItem?): String? {
        val uri = item?.mediaMetadata?.artworkUri ?: return null
        if (uri.scheme != "content" || uri.authority != context.packageName + ".artwork") return null
        return uri.lastPathSegment?.takeIf { it.isNotEmpty() }
    }

    /** JSONObject -> Map mit echten `null`-Werten (statt JSONObject.NULL). */
    private fun JSONObject.toMap(): Map<String, Any?> {
        val out = HashMap<String, Any?>()
        val it = keys()
        while (it.hasNext()) {
            val k = it.next()
            val v = opt(k)
            out[k] = if (v == null || v == JSONObject.NULL) null else v
        }
        return out
    }

    /** optString liefert bei null den Text "null" – das wollen wir nie. */
    private fun JSONObject.optStringOrNull(key: String): String? {
        if (isNull(key)) return null
        return optString(key, "").takeIf { it.isNotEmpty() }
    }
}
