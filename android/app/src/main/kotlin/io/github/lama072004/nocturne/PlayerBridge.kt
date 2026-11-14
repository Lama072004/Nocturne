package io.github.lama072004.nocturne

import android.content.ComponentName
import android.content.Context
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture
import io.flutter.plugin.common.BinaryMessenger
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.CancellationException
import java.util.concurrent.ExecutionException

/**
 * Brücke zwischen der Flutter-UI und dem PlaybackService.
 *
 * WARUM ein MediaController statt direkt auf den ExoPlayer zuzugreifen?
 * Die UI ist nur EIN Client der MediaSession – genau wie die Benachrichtigung,
 * Android Auto oder die Smartwatch. Wenn alle denselben Weg gehen, gibt es nur
 * eine Wahrheit über den Wiedergabezustand. Außerdem kann die Activity
 * beendet werden, während der Service weiterspielt; der Controller verbindet
 * sich beim nächsten Öffnen einfach neu.
 *
 * Kanäle:
 *  - MethodChannel "musicplayer/player"        : Befehle von Dart (play, setQueue …)
 *  - EventChannel  "musicplayer/player_events" : Zustandsänderungen an Dart
 *
 * SICHERHEIT: Die MediaItems enthalten KEINE Adressen mit Anmeldedaten mehr,
 * nur IDs (siehe [MediaItems.fromQueueEntry]). Alles, was in einem MediaItem
 * steht, können andere verbundene Apps über die Session lesen.
 *
 * HÖRSTATISTIK: Gezählt wird NICHT hier und nicht in Dart, sondern im
 * PlaybackService (siehe dort "Hörzeit"): Die Oberfläche kann beendet sein,
 * während die Musik weiterläuft.
 *
 * AKKU: Wir schicken KEINE Position im Sekundentakt. Dart bekommt nur bei
 * echten Änderungen (Play/Pause, Titelwechsel, Seek) einen Schnappschuss mit
 * Zeitstempel und rechnet die aktuelle Position selbst hoch. Das spart
 * dauerndes Aufwecken über die Plattform-Brücke.
 */
class PlayerBridge(private val context: Context, messenger: BinaryMessenger) {

    private val methods = MethodChannel(messenger, "musicplayer/player")
    private val events = EventChannel(messenger, "musicplayer/player_events")
    private val main = Handler(Looper.getMainLooper())

    private var controllerFuture: ListenableFuture<MediaController>? = null
    private var controller: MediaController? = null
    private var eventSink: EventChannel.EventSink? = null

    // Befehle, die eintreffen, bevor der Controller verbunden ist (z. B. direkt
    // nach dem App-Start), werden gepuffert statt verworfen.
    private val pending = mutableListOf<(MediaController) -> Unit>()

    private val listener = object : Player.Listener {
        // onEvents bündelt mehrere gleichzeitige Änderungen zu EINEM Aufruf ->
        // nur ein Event an Dart statt mehrerer.
        override fun onEvents(player: Player, events: Player.Events) {
            if (events.containsAny(
                    Player.EVENT_IS_PLAYING_CHANGED,
                    Player.EVENT_PLAYBACK_STATE_CHANGED,
                    Player.EVENT_MEDIA_ITEM_TRANSITION,
                    Player.EVENT_POSITION_DISCONTINUITY,
                    Player.EVENT_TIMELINE_CHANGED,
                )
            ) {
                sendState()
            }
        }
    }

    fun attach() {
        methods.setMethodCallHandler(::onMethodCall)
        events.setStreamHandler(object : EventChannel.StreamHandler {
            override fun onListen(arguments: Any?, sink: EventChannel.EventSink) {
                eventSink = sink
                // Ab jetzt führt Dart die Warteschlange nach (siehe
                // PlaybackService.uiAttached).
                PlaybackService.uiAttached = true
                sendState()
            }

            override fun onCancel(arguments: Any?) {
                eventSink = null
                PlaybackService.uiAttached = false
            }
        })

        val token = SessionToken(context, ComponentName(context, PlaybackService::class.java))
        val future = MediaController.Builder(context, token).buildAsync()
        controllerFuture = future
        future.addListener({
            // Inzwischen abgebaut (Activity zerstört)? Dann gehört dieses
            // Ergebnis niemandem mehr; releaseFuture hat schon aufgeräumt.
            if (controllerFuture !== future) return@addListener
            // get() wirft, wenn die Verbindung abgebrochen wurde (Activity vor
            // der Verbindung zerstört -> releaseFuture bricht ab) oder der
            // Dienst die Verbindung abgelehnt hat. Ohne try/catch wäre das ein
            // Absturz der ganzen App.
            val c = try {
                future.get()
            } catch (e: CancellationException) {
                null
            } catch (e: ExecutionException) {
                Log.w(TAG, "Verbindung zum Wiedergabe-Dienst fehlgeschlagen", e)
                null
            } catch (e: InterruptedException) {
                null
            }
            if (c == null) {
                // Gepufferte Befehle können nie mehr ausgeführt werden.
                pending.clear()
                return@addListener
            }
            controller = c
            c.addListener(listener)
            pending.forEach { it(c) }
            pending.clear()
            sendState()
        }, ContextCompat.getMainExecutor(context))
    }

    fun detach() {
        controller?.removeListener(listener)
        controllerFuture?.let { MediaController.releaseFuture(it) }
        controllerFuture = null
        controller = null
        pending.clear()
        eventSink = null
        // Oberfläche weg -> der Dienst führt die Warteschlange selbst nach.
        PlaybackService.uiAttached = false
        methods.setMethodCallHandler(null)
        events.setStreamHandler(null)
    }

    private fun withController(block: (MediaController) -> Unit) {
        val c = controller
        if (c != null) block(c) else pending += block
    }

    private fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "setQueue" -> {
                val items = parseItems(call.argument<List<Map<String, Any?>>>("items"))
                val index = call.argument<Int>("index") ?: 0
                val play = call.argument<Boolean>("play") ?: true
                withController { c ->
                    if (items.isEmpty()) {
                        // Leere Liste: anhalten statt IllegalSeekPositionException.
                        c.stop()
                        c.clearMediaItems()
                        return@withController
                    }
                    c.setMediaItems(items, index.coerceIn(0, items.lastIndex), 0L)
                    c.prepare()
                    if (play) c.play()
                }
                result.success(null)
            }

            "updateQueue" -> {
                val items = parseItems(call.argument<List<Map<String, Any?>>>("items"))
                val index = call.argument<Int>("index") ?: 0
                withController { c -> updateQueueKeepingCurrent(c, items, index) }
                result.success(null)
            }

            "play" -> { withController { it.play() }; result.success(null) }
            "pause" -> { withController { it.pause() }; result.success(null) }
            // seekToNext/Previous statt Next/PreviousMediaItem: "Zurück" springt
            // wie bei Spotify erst an den Titelanfang, wenn > 3 s gespielt wurden.
            "next" -> { withController { it.seekToNext() }; result.success(null) }
            "previous" -> { withController { it.seekToPrevious() }; result.success(null) }
            "seekTo" -> {
                val ms = (call.argument<Number>("positionMs") ?: 0).toLong()
                withController { it.seekTo(ms) }
                result.success(null)
            }
            "setRepeatMode" -> {
                // "Titel wiederholen" und "alle wiederholen" macht ExoPlayer
                // selbst. So bleibt auch dabei die Wiedergabe lückenlos.
                val mode = when (call.argument<String>("mode")) {
                    "one" -> Player.REPEAT_MODE_ONE
                    "all" -> Player.REPEAT_MODE_ALL
                    else -> Player.REPEAT_MODE_OFF
                }
                withController { it.repeatMode = mode }
                result.success(null)
            }
            "dspChanged" -> {
                // Klang-Einstellungen wurden in Rust geändert. Der Dienst
                // muss Hardware-Offload passend ein- oder ausschalten.
                withController {
                    it.sendCustomCommand(
                        SessionCommand(PlaybackService.CMD_DSP_CHANGED, Bundle.EMPTY),
                        Bundle.EMPTY,
                    )
                }
                result.success(null)
            }
            "setCrossfade" -> {
                val args = Bundle().apply {
                    putInt(PlaybackService.ARG_SECONDS, call.argument<Int>("seconds") ?: 0)
                }
                withController {
                    it.sendCustomCommand(
                        SessionCommand(PlaybackService.CMD_CROSSFADE, Bundle.EMPTY), args,
                    )
                }
                result.success(null)
            }
            "setSleepTimer" -> {
                // Der Timer selbst läuft im PlaybackService (siehe dort, warum).
                val args = Bundle().apply {
                    putInt(PlaybackService.ARG_MINUTES, call.argument<Int>("minutes") ?: 0)
                }
                withController {
                    it.sendCustomCommand(
                        SessionCommand(PlaybackService.CMD_SLEEP_TIMER, Bundle.EMPTY), args,
                    )
                }
                result.success(null)
            }
            "skipTo" -> {
                val index = call.argument<Int>("index") ?: 0
                withController { c ->
                    // Die Liste in Dart kann kurz anders lang sein als die im
                    // Player -> Index begrenzen statt Ausnahme.
                    val count = c.mediaItemCount
                    if (count > 0) c.seekTo(index.coerceIn(0, count - 1), 0L)
                }
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    /**
     * Ersetzt die Playlist, OHNE den gerade laufenden Titel anzufassen.
     *
     * WARUM so umständlich und nicht einfach setMediaItems()?
     * setMediaItems() würde den aktuellen Titel neu laden -> hörbarer Aussetzer,
     * erneuter Netzwerk-Request. Wenn der Nutzer nur "Zur Warteschlange" drückt
     * oder etwas verschiebt, soll die Musik nahtlos weiterlaufen. Deshalb
     * tauschen wir nur die Einträge VOR und NACH dem aktuellen Titel aus.
     *
     * WARUM wird der laufende Titel per mediaId GESUCHT statt [index] zu
     * vertrauen? Dart schickt den Index, den es zuletzt kannte. ExoPlayer kann
     * inzwischen von selbst zum nächsten Titel gesprungen sein (Titelende,
     * Kopfhörer-Taste). Würden wir dann den alten Index setzen, spränge die
     * Wiedergabe hörbar auf den vorigen Titel zurück. Die mediaId ist eine
     * eindeutige Queue-Eintrags-ID aus Rust, die Suche ist also eindeutig.
     */
    private fun updateQueueKeepingCurrent(c: MediaController, items: List<MediaItem>, index: Int) {
        if (items.isEmpty()) {
            c.stop()
            c.clearMediaItems()
            return
        }
        val current = c.currentMediaItem
        val currentIdx = c.currentMediaItemIndex
        // Erst der schnelle Weg (Index passt), sonst suchen.
        val keep = when {
            current == null -> -1
            items.getOrNull(index)?.mediaId == current.mediaId -> index
            else -> items.indexOfFirst { it.mediaId == current.mediaId }
        }
        if (keep < 0 || currentIdx !in 0 until c.mediaItemCount) {
            // Der laufende Titel kommt in der neuen Liste gar nicht vor (oder
            // es läuft nichts) -> kompletter Austausch ab dem gewünschten Eintrag.
            c.setMediaItems(items, index.coerceIn(0, items.lastIndex), 0L)
            c.prepare()
            return
        }
        // 1) Alles NACH dem aktuellen Titel ersetzen.
        if (currentIdx + 1 < c.mediaItemCount) {
            c.removeMediaItems(currentIdx + 1, c.mediaItemCount)
        }
        if (keep + 1 < items.size) c.addMediaItems(items.subList(keep + 1, items.size))
        // 2) Alles VOR dem aktuellen Titel ersetzen (Verlauf für "Zurück").
        if (currentIdx > 0) c.removeMediaItems(0, currentIdx)
        if (keep > 0) c.addMediaItems(0, items.subList(0, keep))
    }

    /**
     * Dart-Maps -> MediaItems. Die eigentliche Umwandlung liegt in
     * [MediaItems.fromQueueEntry], damit der Dienst Einträge aus Rust-JSON
     * (Android Auto, Widget, Fortsetzen) GENAU gleich baut.
     */
    private fun parseItems(raw: List<Map<String, Any?>>?): List<MediaItem> =
        MediaItems.fromDartList(context, raw)

    private fun sendState() {
        val sink = eventSink ?: return
        val c = controller ?: return
        val state = mapOf(
            "isPlaying" to c.isPlaying,
            "isBuffering" to (c.playbackState == Player.STATE_BUFFERING),
            "index" to c.currentMediaItemIndex,
            "entryId" to c.currentMediaItem?.mediaId,
            "positionMs" to c.currentPosition,
            "durationMs" to c.duration.takeIf { it > 0 },
            // Zeitpunkt der Messung, damit Dart die Position hochrechnen kann.
            "timestampMs" to System.currentTimeMillis(),
        )
        main.post { sink.success(state) }
    }

    private companion object {
        const val TAG = "PlayerBridge"
    }
}
