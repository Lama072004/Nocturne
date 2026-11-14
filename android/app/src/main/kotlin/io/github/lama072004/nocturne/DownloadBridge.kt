package io.github.lama072004.nocturne

import android.content.Context
import android.net.Uri
import android.os.Handler
import android.os.Looper
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.offline.DownloadRequest
import androidx.media3.exoplayer.offline.DownloadService
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

/**
 * Brücke zwischen Flutter und dem Download-Dienst.
 *
 * Kanäle:
 *  - MethodChannel "musicplayer/downloads"        : start, remove, removeAll,
 *                                                   list, configure, storageInfo
 *  - EventChannel  "musicplayer/download_events"  : "es hat sich etwas geändert"
 *
 * AKKU: Über den EventChannel kommt nur ein Signal bei Statuswechseln
 * (gestartet, fertig, fehlgeschlagen, entfernt). Den laufenden Fortschritt in
 * Prozent fragt Dart selbst per `list` ab – und zwar nur, solange der
 * Downloads-Tab sichtbar ist.
 */
@OptIn(UnstableApi::class)
class DownloadBridge(private val context: Context, messenger: BinaryMessenger) {

    private val methods = MethodChannel(messenger, "musicplayer/downloads")
    private val events = EventChannel(messenger, "musicplayer/download_events")
    private val main = Handler(Looper.getMainLooper())

    // Der Download-Index ist eine SQLite-Datenbank. Abfragen laufen deshalb
    // nicht auf dem UI-Thread, sonst könnte die Oberfläche kurz ruckeln.
    // Gemeinsamer Thread aus MediaCache: Ein eigener Executor je Bridge
    // würde nie geschlossen und bei jedem Neuaufbau der Activity einen
    // Thread zurücklassen.
    private val io = MediaCache.io
    private var sink: EventChannel.EventSink? = null

    /**
     * `false` nach [detach]. Antworten aus dem Hintergrund-Thread, die erst
     * danach fertig werden, würden sonst bei einer abgebauten Flutter-Engine
     * landen.
     */
    @Volatile
    private var attached = false

    private val listener = object : DownloadManager.Listener {
        override fun onDownloadChanged(
            manager: DownloadManager,
            download: Download,
            finalException: Exception?,
        ) = notifyChanged()

        override fun onDownloadRemoved(manager: DownloadManager, download: Download) =
            notifyChanged()
    }

    private fun notifyChanged() {
        main.post { if (attached) sink?.success("changed") }
    }

    /** Stellt eine Antwort auf dem Haupt-Thread zu – aber nur, solange verbunden. */
    private fun reply(block: () -> Unit) {
        main.post { if (attached) block() }
    }

    fun attach() {
        attached = true
        methods.setMethodCallHandler(::onMethodCall)
        events.setStreamHandler(object : EventChannel.StreamHandler {
            override fun onListen(arguments: Any?, s: EventChannel.EventSink) { sink = s }
            override fun onCancel(arguments: Any?) { sink = null }
        })
        MediaCache.downloadManager(context).addListener(listener)
    }

    fun detach() {
        attached = false
        MediaCache.downloadManager(context).removeListener(listener)
        methods.setMethodCallHandler(null)
        events.setStreamHandler(null)
        sink = null
    }

    /**
     * Schickt einen Auftrag an den Download-Dienst.
     *
     * WARUM so umständlich? `foreground = false` nutzt `startService()`. Ab
     * Android 8 wirft das eine IllegalStateException, wenn die App gerade im
     * Hintergrund ist – genau der Fall beim Auto-Sync nach einem WLAN-Wechsel.
     * Dann versuchen wir es als Foreground-Service (DownloadService zeigt dafür
     * sofort seine Benachrichtigung). Ab Android 12 kann auch das aus dem
     * Hintergrund verboten sein (ForegroundServiceStartNotAllowedException);
     * dann bekommt Dart einen Fehler und kann es später erneut versuchen,
     * statt dass die App abstürzt.
     */
    private fun sendToService(result: MethodChannel.Result, send: (foreground: Boolean) -> Unit) {
        try {
            send(false)
            result.success(null)
            return
        } catch (e: IllegalStateException) {
            // App im Hintergrund -> unten mit Vordergrund-Start erneut.
        } catch (e: RuntimeException) {
            result.error("service_start_failed", e.message, null)
            return
        }
        try {
            send(true)
            result.success(null)
        } catch (e: RuntimeException) {
            result.error("service_start_failed", e.message, null)
        }
    }

    private fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        val service = MusicDownloadService::class.java
        when (call.method) {
            "configure" -> {
                // Einstellungen aus Rust/Dart hier ablegen, damit der Dienst
                // sie auch ohne laufende UI kennt (siehe MediaCache.prefs).
                MediaCache.prefs(context).edit()
                    .putBoolean(MediaCache.PREF_WIFI_ONLY, call.argument<Boolean>("wifiOnly") ?: true)
                    .putBoolean(MediaCache.PREF_SD_CARD, call.argument<Boolean>("sdCard") ?: false)
                    .putInt(MediaCache.PREF_STREAM_CACHE_MB, call.argument<Int>("streamCacheMb") ?: 512)
                    .apply()
                // "Nur im WLAN" wirkt sofort, auch auf laufende Downloads.
                MediaCache.downloadManager(context).requirements = MediaCache.requirements(context)
                result.success(null)
            }

            "start" -> {
                val id = call.argument<String>("id")
                val url = call.argument<String>("url")
                if (id.isNullOrEmpty() || url.isNullOrEmpty()) {
                    result.error("bad_args", "id und url sind Pflicht", null)
                    return
                }
                val request = DownloadRequest.Builder(id, Uri.parse(url))
                    // Gleicher Schlüssel wie beim Abspielen (Song-ID). Nur so
                    // findet der Player die heruntergeladene Datei wieder.
                    .setCustomCacheKey(id)
                    .build()
                sendToService(result) { fg ->
                    DownloadService.sendAddDownload(context, service, request, fg)
                }
            }

            "remove" -> {
                val id = call.argument<String>("id")
                if (id.isNullOrEmpty()) {
                    result.error("bad_args", "id ist Pflicht", null)
                    return
                }
                sendToService(result) { fg ->
                    DownloadService.sendRemoveDownload(context, service, id, fg)
                }
            }

            "removeAll" -> sendToService(result) { fg ->
                DownloadService.sendRemoveAllDownloads(context, service, fg)
            }

            // Wo liegen die Downloads wirklich? (siehe MediaCache.PREF_ACTIVE_DIR)
            "storageInfo" -> io.execute {
                try {
                    val info = MediaCache.storageInfo(context)
                    reply { result.success(info) }
                } catch (e: Exception) {
                    reply { result.error("storage_info_failed", e.message, null) }
                }
            }

            "list" -> io.execute {
                val out = ArrayList<Map<String, Any?>>()
                try {
                    val manager = MediaCache.downloadManager(context)
                    // Laufende Downloads kennt der Manager mit aktuellem
                    // Fortschritt; der Index auf der Platte hinkt etwas nach.
                    val live = manager.currentDownloads.associateBy { it.request.id }
                    manager.downloadIndex.getDownloads().use { cursor ->
                        while (cursor.moveToNext()) {
                            val d = live[cursor.download.request.id] ?: cursor.download
                            out += mapOf(
                                "id" to d.request.id,
                                "state" to stateName(d.state),
                                "percent" to d.percentDownloaded.takeIf { it >= 0f }?.toDouble(),
                                "bytes" to d.bytesDownloaded,
                            )
                        }
                    }
                    reply { result.success(out) }
                } catch (e: Exception) {
                    reply { result.error("list_failed", e.message, null) }
                }
            }

            else -> result.notImplemented()
        }
    }

    /** Namen statt Zahlen: Dart muss dann keine Media3-Konstanten kennen. */
    private fun stateName(state: Int): String = when (state) {
        Download.STATE_COMPLETED -> "completed"
        Download.STATE_DOWNLOADING, Download.STATE_RESTARTING -> "downloading"
        Download.STATE_FAILED -> "failed"
        Download.STATE_REMOVING -> "removing"
        // QUEUED und STOPPED: wartet (z. B. auf WLAN).
        else -> "queued"
    }
}
