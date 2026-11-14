package io.github.lama072004.nocturne

import android.content.BroadcastReceiver
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.core.content.ContextCompat
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.session.MediaController
import androidx.media3.session.SessionToken
import com.google.common.util.concurrent.ListenableFuture

/**
 * Empfängt die Knöpfe der Homescreen-Widgets (siehe HomeWidgets.kt).
 * Nicht exportiert: Nur unsere eigenen PendingIntents erreichen ihn.
 *
 * Zwei Fälle:
 *  1. Der PlaybackService läuft schon in diesem Prozess -> Befehl direkt an
 *     ihn ([PlaybackService.handleWidgetCommand]). Kein Verbindungsaufbau.
 *  2. Kein Dienst -> wir verbinden uns per MediaController. Das erzeugt den
 *     Dienst; danach geht der Befehl wie in Fall 1 an ihn. Beginnt die
 *     Wiedergabe, macht Media3 den Dienst selbst zum Foreground-Service.
 *
 * WARUM nicht androidx.media3.session.MediaButtonReceiver für die Knöpfe?
 * Der leitet ab Android 8 nur "Play"-Tasten weiter (Weiter/Zurück verwirft
 * er absichtlich, um ForegroundServiceDidNotStartInTimeException zu
 * vermeiden) und kann kein "spiele Album X". Er ist trotzdem im Manifest
 * eingetragen – für Bluetooth-Tasten und das Fortsetzen nach Neustart über
 * die Systemoberfläche.
 *
 * Android 12+: Ein Klick auf ein Widget erlaubt der App kurzzeitig, einen
 * Foreground-Service zu starten. Genau in diesem Fenster startet hier die
 * Wiedergabe.
 */
class WidgetActionReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        val action = intent.action ?: return
        if (action !in ACTIONS) return
        val mediaId = intent.getStringExtra(HomeWidgets.EXTRA_MEDIA_ID)

        // Fall 1: Dienst läuft (onReceive läuft auf dem Haupt-Thread, wie der Player).
        PlaybackService.running()?.let {
            it.handleWidgetCommand(action, mediaId)
            return
        }
        // Ohne Dienst gibt es nichts zu überspringen.
        if (action == HomeWidgets.ACTION_NEXT || action == HomeWidgets.ACTION_PREVIOUS) return

        // Fall 2: Dienst über eine Controller-Verbindung erzeugen.
        // applicationContext: Aus dem Receiver-Kontext darf man sich nicht an
        // einen Dienst binden (ReceiverCallNotAllowedException).
        val pending = goAsync()
        connectAndRun(context.applicationContext, action, mediaId) { pending.finish() }
    }

    private companion object {
        const val TAG = "WidgetAction"

        val ACTIONS = setOf(
            HomeWidgets.ACTION_PLAY_PAUSE,
            HomeWidgets.ACTION_NEXT,
            HomeWidgets.ACTION_PREVIOUS,
            HomeWidgets.ACTION_PLAY_ID,
        )

        /**
         * So lange halten wir die Verbindung höchstens, falls die Wiedergabe
         * nie beginnt (z. B. nichts gespeichert, Server nicht erreichbar).
         * Eine EINMALIGE Frist pro Klick, kein wiederkehrender Timer.
         */
        const val HOLD_MS = 30_000L

        val main = Handler(Looper.getMainLooper())

        /** Die gerade gehaltene Verbindung (nur Haupt-Thread). */
        var held: ListenableFuture<MediaController>? = null

        fun release(future: ListenableFuture<MediaController>) {
            if (held === future) held = null
            MediaController.releaseFuture(future)
        }

        fun connectAndRun(app: Context, action: String, mediaId: String?, done: () -> Unit) {
            // Ein älterer, noch gehaltener Klick wird ersetzt.
            held?.let { release(it) }
            val token = SessionToken(app, ComponentName(app, PlaybackService::class.java))
            val future = MediaController.Builder(app, token).buildAsync()
            held = future
            future.addListener({
                val controller = try {
                    future.get()
                } catch (e: Exception) {
                    // Abgebrochen oder vom Dienst abgelehnt: still aufgeben.
                    Log.w(TAG, "Keine Verbindung zum Wiedergabe-Dienst", e)
                    null
                }
                if (controller == null) {
                    release(future)
                    done()
                    return@addListener
                }
                val service = PlaybackService.running()
                if (service != null) {
                    service.handleWidgetCommand(action, mediaId)
                } else if (action == HomeWidgets.ACTION_PLAY_ID && !mediaId.isNullOrEmpty()) {
                    // Rückfall (Dienst in anderem Prozess – sollte nicht vorkommen):
                    // Ein Item ohne songId lässt der Dienst von Rust auflösen.
                    controller.setMediaItem(MediaItem.Builder().setMediaId(mediaId).build())
                    controller.prepare()
                    controller.play()
                } else {
                    // "Play" bei leerer Liste -> Media3 ruft onPlaybackResumption.
                    controller.play()
                }
                done()
                // Verbindung halten, bis Musik läuft (dann hält Media3 den
                // Dienst als Foreground-Service am Leben) – sonst würde der nur
                // gebundene Dienst beim Trennen sofort wieder beendet, mitten
                // im Laden der Warteschlange.
                if (controller.isPlaying) {
                    release(future)
                    return@addListener
                }
                controller.addListener(object : Player.Listener {
                    override fun onIsPlayingChanged(isPlaying: Boolean) {
                        if (isPlaying) release(future)
                    }
                })
                main.postDelayed({ if (held === future) release(future) }, HOLD_MS)
            }, ContextCompat.getMainExecutor(app))
        }
    }
}
