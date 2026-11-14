package io.github.lama072004.nocturne

import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.media.AudioDeviceCallback
import android.media.AudioDeviceInfo
import android.media.AudioManager
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.os.SystemClock
import android.util.Log
import androidx.annotation.OptIn
import androidx.media3.common.AudioAttributes
import androidx.media3.common.C
import androidx.media3.common.MediaItem
import androidx.media3.common.MediaMetadata
import androidx.media3.common.PlaybackException
import androidx.media3.common.Player
import androidx.media3.common.TrackSelectionParameters
import androidx.media3.common.util.UnstableApi
import androidx.media3.common.util.Util
import androidx.media3.exoplayer.DefaultRenderersFactory
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.DefaultAudioSink
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.session.LibraryResult
import androidx.media3.session.MediaLibraryService
import androidx.media3.session.MediaSession
import androidx.media3.session.SessionCommand
import androidx.media3.session.SessionError
import androidx.media3.session.SessionResult
import androidx.core.content.ContextCompat
import com.google.common.collect.ImmutableList
import com.google.common.util.concurrent.FutureCallback
import com.google.common.util.concurrent.Futures
import com.google.common.util.concurrent.ListenableFuture
import com.google.common.util.concurrent.MoreExecutors
import java.io.IOException
import java.lang.ref.WeakReference
import java.util.Locale

/**
 * Der zentrale Wiedergabe-Dienst der App.
 *
 * WARUM ein MediaLibraryService (und nicht nur ein normaler Service)?
 * Android erkennt eine App nur dann zuverlässig als "Musik-App", wenn sie eine
 * MediaSession über einen MediaLibraryService/MediaBrowserService anbietet.
 * Daran hängen:
 *  - die Medien-Benachrichtigung und die Steuerung auf dem Sperrbildschirm,
 *  - die Medien-Kachel in den Schnelleinstellungen und der Ausgabe-Umschalter,
 *  - Bluetooth-/Kopfhörer-Tasten,
 *  - "Weiterspielen" nach einem Neustart (Media Resumption),
 *  - Android Auto, Wear OS und Sprachassistenten (über den Library-Baum).
 * Media3 erledigt dabei automatisch den Foreground-Service mit dem Typ
 * "mediaPlayback" und die Benachrichtigung – wir müssen nichts davon selbst bauen.
 *
 * WARUM liegt die Warteschlangen-LOGIK nicht hier?
 * Die Spotify-Logik (eigene Warteschlange vor dem Kontext, Verschieben usw.)
 * lebt im Rust-Kern. ExoPlayer bekommt nur das fertige, "flachgeklopfte"
 * Ergebnis als Playlist. So kann ExoPlayer trotzdem lückenlos (gapless) den
 * nächsten Titel vorpuffern, und die Benachrichtigung kennt "Weiter/Zurück".
 *
 * OHNE FLUTTER-OBERFLÄCHE: Der Dienst ist eine vollständige Musik-App für
 * sich. Er
 *  - liefert den Inhaltsbaum und die Suche für Android Auto/Assistenten
 *    (über Rust: nativeBrowse/nativeSearch),
 *  - baut Warteschlangen für fremde Controller und Widgets selbst
 *    (nativePlayFromMediaId) und setzt nach einem Neustart fort (nativeResume),
 *  - zählt die gehörte Zeit und meldet Verlauf/Scrobbling (nativeReportPlay),
 *  - meldet die Netzart an Rust (WLAN/mobil) und
 *  - versorgt die Homescreen-Widgets (siehe HomeWidgets.kt).
 *
 * SICHERHEIT: Der Dienst ist exportiert (Android Auto & Co. müssen ihn
 * erreichen). Deshalb
 *  - lässt [LibraryCallback.onConnect] nur bekannte Controller zu, und
 *  - enthalten die MediaItems in der Session keine Adressen mit Token; die
 *    Stream-URL baut erst [resolve] hier im Prozess aus der Song-ID.
 */
@OptIn(UnstableApi::class)
class PlaybackService : MediaLibraryService() {

    private var session: MediaLibrarySession? = null
    private lateinit var player: ExoPlayer
    private val handler = Handler(Looper.getMainLooper())

    /**
     * Gemeinsamer Hintergrund-Thread für die Auflösung der MediaItems
     * (Download-Index = SQLite, JNI nach Rust). Nie auf dem Haupt-Thread:
     * Eine lange Warteschlange würde sonst die Session kurz blockieren.
     */
    private val background = MoreExecutors.listeningDecorator(MediaCache.io)

    /**
     * Für Rust-Aufrufe, die ins Netz gehen können (Bibliothek, Suche,
     * Warteschlange bauen, Scrobbeln). Getrennt von [background], damit ein
     * langsamer Server nie das Auflösen der eigenen Titel aufhält.
     */
    private val net = NativeCore.executor

    /** Sprache für die Ordnernamen, die Rust im Inhaltsbaum vergibt. */
    private val german: Boolean get() = Locale.getDefault().language == "de"

    override fun onCreate() {
        super.onCreate()

        // Rust-Kern einrichten, falls die Flutter-Oberfläche (noch) nicht
        // läuft, z. B. beim Start über Android Auto. `filesDir` ist derselbe
        // Ordner, den Dart über path_provider an Rust übergibt; der Schlüssel
        // entschlüsselt die gespeicherten Anmeldedaten (siehe SecretStore).
        NativeCore.ensureInit(this)

        // DownloadManager hier auf dem Haupt-Thread anlegen: Er bindet seine
        // Rückrufe an den Thread, auf dem er entsteht. [resolve] fragt ihn
        // später vom Hintergrund-Thread aus.
        MediaCache.downloadManager(this)

        // Crossfade-Dauer vom letzten Mal (der Dienst kann ohne Oberfläche
        // starten, z. B. über Android Auto oder ein Widget).
        crossfade.crossfadeMs = playbackPrefs().getInt(PREF_CROSSFADE_MS, 0)

        // Eigene Renderers-Factory: An der Stelle, an der ExoPlayer seine
        // Audio-Ausgabe baut, hängen wir die Rust-Effektkette ein.
        val renderersFactory = object : DefaultRenderersFactory(this) {
            override fun buildAudioSink(
                context: Context,
                enableFloatOutput: Boolean,
                enableAudioTrackPlaybackParams: Boolean,
            ): AudioSink = crossfade.wrap(
                DefaultAudioSink.Builder(context)
                    // Reihenfolge: erst die Effekte (Lautstärke-Angleich je
                    // Titel, Equalizer), DANN überblenden. So wird der
                    // Ausklang mit den Werten SEINES Titels bearbeitet.
                    .setAudioProcessors(arrayOf(RustDspProcessor(), crossfade))
                    .build(),
            )
        }

        player = ExoPlayer.Builder(this, renderersFactory)
            // Alle Titel laufen über MediaCache: erst Download, dann
            // Streaming-Cache, dann Netz. Dadurch funktioniert Offline-
            // Wiedergabe ohne Sonderfall im Player.
            .setMediaSourceFactory(
                DefaultMediaSourceFactory(this)
                    .setDataSourceFactory(MediaCache.playbackDataSourceFactory(this)),
            )
            // Musik-Attribute + handleAudioFocus=true: ExoPlayer pausiert selbst,
            // wenn z. B. ein Anruf kommt, und duckt bei Navigationsansagen.
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(C.USAGE_MEDIA)
                    .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                    .build(),
                /* handleAudioFocus = */ true,
            )
            // Kopfhörer ausgesteckt / Bluetooth getrennt -> pausieren statt laut
            // aus dem Handy-Lautsprecher weiterspielen.
            .setHandleAudioBecomingNoisy(true)
            // WAKE_MODE_NETWORK hält CPU + WLAN wach, aber NUR solange tatsächlich
            // gespielt wird. Ohne das bricht das Streaming bei ausgeschaltetem
            // Bildschirm ab; ein dauerhafter Wakelock würde dagegen Akku fressen.
            .setWakeMode(C.WAKE_MODE_NETWORK)
            .build()

        applyDspState()
        watchOutputDevice()

        player.addListener(object : Player.Listener {
            override fun onMediaItemTransition(mediaItem: MediaItem?, reason: Int) {
                // Hörzeit: Neuer Eintrag -> den alten abschließen. Ausnahme:
                // Nur die Liste wurde ersetzt, der laufende Eintrag ist aber
                // derselbe (Dart schickt eine neue Warteschlange) – dann läuft
                // die Zählung weiter. Bei "Titel wiederholen" (REPEAT) zählt
                // jeder Durchlauf einzeln.
                if (reason == Player.MEDIA_ITEM_TRANSITION_REASON_REPEAT ||
                    mediaItem?.mediaId != statEntryId
                ) {
                    statsFinish()
                    statsStart(mediaItem)
                }
                syncQueueWithoutUi(mediaItem, reason)

                // Neuer Titel -> seine ReplayGain-Werte an die Effektkette.
                // Sie reisen im MediaItem mit (siehe PlayerBridge.parseItems).
                // BEKANNTE UNSCHÄRFE: Dieses Ereignis kommt, wenn der neue
                // Titel HÖRBAR wird. Die ersten Millisekunden (Tiefe des
                // AudioTrack-Puffers) sind da schon mit dem Gain des vorigen
                // Titels verarbeitet worden.
                val extras = mediaItem?.requestMetadata?.extras
                if (NativeDsp.available) {
                    NativeDsp.nativeSetTrackGain(
                        extras?.getFloat(EXTRA_TRACK_GAIN, Float.NaN) ?: Float.NaN,
                        extras?.getFloat(EXTRA_ALBUM_GAIN, Float.NaN) ?: Float.NaN,
                        extras?.getFloat(EXTRA_TRACK_PEAK, Float.NaN) ?: Float.NaN,
                        extras?.getFloat(EXTRA_ALBUM_PEAK, Float.NaN) ?: Float.NaN,
                    )
                }
            }

            override fun onPlayWhenReadyChanged(playWhenReady: Boolean, reason: Int) {
                // Sleep-Timer "bis Titelende" hat ausgelöst -> wieder normal
                // schalten, sonst würde JEDER weitere Titel am Ende pausieren.
                if (reason == Player.PLAY_WHEN_READY_CHANGE_REASON_END_OF_MEDIA_ITEM) {
                    player.pauseAtEndOfMediaItems = false
                }
                if (playWhenReady) onPlaybackResumedForSleepTimer()
                // Pause -> Position für "Weiterspielen nach Neustart" sichern.
                // Ereignisgesteuert statt per Timer (Akku).
                else savePosition()
            }

            override fun onPlaybackStateChanged(playbackState: Int) {
                when (playbackState) {
                    // Läuft wieder -> der nächste Netzfehler ist ein neuer Fall.
                    Player.STATE_READY -> {
                        retriedOnSameNetwork = false
                        statsNoteDuration()
                    }
                    // Nutzer oder Controller hat selbst neu gestartet -> nicht
                    // mehr auf das Netz warten.
                    Player.STATE_BUFFERING -> stopWaitingForNetwork()
                    // Ende der Warteschlange oder Stopp (IDLE ohne Fehler):
                    // Der Eintrag ist "fertig" -> ggf. melden, Position sichern.
                    // IDLE MIT Fehler (Netz weg) zählt NICHT als Stopp: Nach
                    // dem Fortsetzen geht derselbe Eintrag weiter.
                    Player.STATE_ENDED -> {
                        statsFinish()
                        statsStart(player.currentMediaItem)
                        savePosition()
                    }
                    Player.STATE_IDLE -> if (player.playerError == null) {
                        statsFinish()
                        statsStart(player.currentMediaItem)
                        savePosition()
                    }
                    else -> Unit
                }
            }

            override fun onIsPlayingChanged(isPlaying: Boolean) {
                // Hörzeit nur zählen, solange WIRKLICH Ton kommt (nicht
                // beim Puffern, nicht pausiert).
                if (isPlaying) statsResume() else statsPause()
            }

            override fun onEvents(player: Player, events: Player.Events) {
                // Widgets NUR bei echten Änderungen neu zeichnen; pushWidgetState
                // filtert zusätzlich Ereignisse heraus, die am Bild nichts ändern
                // (z. B. BUFFERING <-> READY).
                if (events.containsAny(
                        Player.EVENT_MEDIA_ITEM_TRANSITION,
                        Player.EVENT_PLAY_WHEN_READY_CHANGED,
                        Player.EVENT_PLAYBACK_STATE_CHANGED,
                        Player.EVENT_MEDIA_METADATA_CHANGED,
                    )
                ) {
                    pushWidgetState()
                }
            }

            override fun onPlayerError(error: PlaybackException) {
                if (error.errorCode in NETWORK_ERRORS && player.playWhenReady) {
                    waitForNetwork()
                }
            }
        })

        // Tippen auf die Benachrichtigung öffnet die App.
        val openApp = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

        session = MediaLibrarySession.Builder(this, player, LibraryCallback())
            .setSessionActivity(openApp)
            .build()

        watchNetworkType()
        instance = WeakReference(this)
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaLibrarySession? =
        session

    /**
     * Wird aufgerufen, wenn der Nutzer die App aus den "Letzten Apps" wischt.
     * Läuft gerade nichts, beenden wir den Dienst sofort, damit kein
     * Hintergrundprozess unnötig Akku verbraucht. Läuft Musik, spielt sie weiter
     * (wie bei Spotify).
     */
    override fun onTaskRemoved(rootIntent: Intent?) {
        // Position sichern: Android kann den Prozess danach jederzeit beenden.
        savePosition()
        if (!player.playWhenReady || player.mediaItemCount == 0) {
            stopSelf()
        }
    }

    override fun onDestroy() {
        instance = null
        // Letzten Eintrag abschließen (Verlauf/Scrobbling) und Position sichern,
        // SOLANGE der Player noch lebt.
        statsFinish()
        savePosition()
        // Widgets: letzter Titel bleibt stehen, aber mit Play-Symbol.
        pushWidgetState(force = true, serviceEnding = true)
        audioManager().unregisterAudioDeviceCallback(deviceCallback)
        stopWaitingForNetwork()
        stopWatchingNetworkType()
        handler.removeCallbacksAndMessages(null)
        session?.release()
        session = null
        player.release()
        super.onDestroy()
    }

    // -----------------------------------------------------------------------
    // Warteschlange nachführen, wenn die Oberfläche nicht läuft
    // -----------------------------------------------------------------------

    /** Index des Eintrags, der VOR dem letzten Wechsel lief (-1 = keiner). */
    private var lastIndex = -1

    /**
     * Sagt Rust, welcher Eintrag jetzt läuft – aber NUR, wenn keine
     * Flutter-Oberfläche verbunden ist ([uiAttached]).
     *
     * WARUM nicht immer? Solange die Oberfläche läuft, führt Dart die
     * Warteschlange nach und schickt bei Bedarf eine neue Liste. Täten das
     * zwei Stellen gleichzeitig, bekäme die zweite ein "hat sich nichts
     * geändert" zurück, und die neue Liste käme nie beim Player an.
     *
     * WARUM überhaupt? Ohne Oberfläche (Musik im Hintergrund, Android Auto,
     * Widget) bliebe Rust auf dem alten Titel stehen: "Weiterspielen" nach
     * einem Neustart begänne beim falschen Titel.
     */
    private fun syncQueueWithoutUi(mediaItem: MediaItem?, reason: Int) {
        val previousIndex = lastIndex
        val index = player.currentMediaItemIndex
        lastIndex = index
        if (uiAttached) return
        // Nur die Liste wurde ersetzt (neue Warteschlange von Auto/Widget):
        // Rust hat sie selbst gebaut und kennt den Stand schon.
        if (reason == Player.MEDIA_ITEM_TRANSITION_REASON_PLAYLIST_CHANGED) return
        val id = mediaItem?.mediaId?.takeIf { it.isNotEmpty() } ?: return
        // "Alle wiederholen": ExoPlayer ist am Ende von selbst nach vorn
        // gesprungen. Nur dann baut Rust die Runde neu auf.
        val wrapped = reason == Player.MEDIA_ITEM_TRANSITION_REASON_AUTO &&
            player.repeatMode == Player.REPEAT_MODE_ALL &&
            index == 0 && previousIndex > 0 &&
            previousIndex == player.mediaItemCount - 1
        val app = applicationContext
        val future = net.submit<MediaSession.MediaItemsWithStartPosition?> {
            NativeCore.ensureInit(app)
            val queue = MediaItems.parseQueueJson(app, NativeCore.transition(id, wrapped))
                ?: return@submit null
            resolveQueue(queue.items, queue.index, 0L)
        }
        Futures.addCallback(
            future,
            object : FutureCallback<MediaSession.MediaItemsWithStartPosition?> {
                override fun onSuccess(result: MediaSession.MediaItemsWithStartPosition?) {
                    // null = Liste im Player stimmt noch (der Normalfall).
                    if (result == null || session == null) return
                    replaceAroundCurrent(result.mediaItems)
                }

                override fun onFailure(t: Throwable) {
                    Log.i(TAG, "Warteschlange nicht nachgeführt: ${t.message}")
                }
            },
            ContextCompat.getMainExecutor(this),
        )
    }

    /**
     * Tauscht alles VOR und NACH dem laufenden Eintrag aus, ohne ihn selbst
     * anzufassen (kein Aussetzer). Gleiche Idee wie
     * PlayerBridge.updateQueueKeepingCurrent, hier direkt am Player.
     */
    private fun replaceAroundCurrent(items: List<MediaItem>) {
        val current = player.currentMediaItem ?: return
        val currentIdx = player.currentMediaItemIndex
        val keep = items.indexOfFirst { it.mediaId == current.mediaId }
        // Der laufende Eintrag fehlt in der neuen Liste: lieber nichts tun,
        // als die Wiedergabe zu unterbrechen. Die Oberfläche gleicht das
        // beim nächsten Öffnen ab.
        if (keep < 0 || currentIdx !in 0 until player.mediaItemCount) return
        if (currentIdx + 1 < player.mediaItemCount) {
            player.removeMediaItems(currentIdx + 1, player.mediaItemCount)
        }
        if (keep + 1 < items.size) player.addMediaItems(items.subList(keep + 1, items.size))
        if (currentIdx > 0) player.removeMediaItems(0, currentIdx)
        if (keep > 0) player.addMediaItems(0, items.subList(0, keep))
        lastIndex = player.currentMediaItemIndex
    }

    // -----------------------------------------------------------------------
    // Weiterspielen nach Neustart: Position sichern
    // -----------------------------------------------------------------------

    /**
     * Meldet die aktuelle Position an Rust (schnell, Haupt-Thread erlaubt).
     * Aufgerufen bei Pause, Stopp, onTaskRemoved und onDestroy – NICHT per
     * Timer. Welcher Eintrag läuft, weiß Rust aus [NativeCore.nowPlaying].
     */
    private fun savePosition() {
        if (player.mediaItemCount == 0) return
        // Ist die Warteschlange zu Ende gespielt, merken wir uns den ANFANG
        // des letzten Titels. Sonst setzte "Play" (Widget, Kopfhörer-Taste,
        // Mini-Player nach App-Neustart) genau am Titelende fort – es käme
        // kein Ton, und der Nutzer hielte die App für kaputt (im Test so
        // passiert).
        val ended = player.playbackState == Player.STATE_ENDED
        val position = if (ended) 0L else player.currentPosition.coerceAtLeast(0L)
        NativeCore.savePosition(position)
    }

    // -----------------------------------------------------------------------
    // Auflösen der MediaItems (Stream-URL, Download-Cache)
    // -----------------------------------------------------------------------

    /**
     * Macht aus einem MediaItem der Oberfläche ein abspielbares Item.
     * Läuft auf dem Hintergrund-Thread (Datenbank + JNI).
     *
     * @return `null`, wenn das Item nicht abspielbar ist: keine Song-ID (z. B.
     *         ein Sprachassistent schickt nur einen Suchbegriff) oder weder
     *         eine Stream-URL noch ein fertiger Download.
     */
    private fun resolve(item: MediaItem): MediaItem? {
        val songId = item.requestMetadata.extras?.getString(EXTRA_SONG_ID)
            ?.takeIf { it.isNotEmpty() } ?: return null
        val downloaded = isDownloadComplete(songId)
        // Die URL kommt aus Rust, NICHT aus dem MediaItem (siehe PlayerBridge:
        // Adressen mit Token dürfen nicht durch die Session reisen).
        NativeCore.ensureInit(this)
        val url = NativeCore.streamUrl(songId)
        val uri = when {
            url != null -> Uri.parse(url)
            // Nicht angemeldet, aber vollständig heruntergeladen: Der Player
            // liest dann alles aus dem Download-Cache und öffnet die Adresse
            // nie. Sie muss nur eindeutig sein.
            downloaded -> Uri.Builder().scheme("nocturne").authority("download")
                .appendPath(songId).build()
            else -> return null
        }
        return item.buildUpon()
            .setUri(uri)
            // Nur bei FERTIGEM Download die Song-ID als Schlüssel: Dann liest
            // der Player aus dem Download-Cache. Ein halber Download würde
            // sonst mit Stream-Bytes anderer Qualität aufgefüllt (siehe
            // MediaCache.playbackDataSourceFactory).
            .setCustomCacheKey(if (downloaded) songId else null)
            .build()
    }

    private fun isDownloadComplete(songId: String): Boolean = try {
        MediaCache.downloadManager(this).downloadIndex.getDownload(songId)?.state ==
            Download.STATE_COMPLETED
    } catch (e: IOException) {
        // Index nicht lesbar -> sicherheitshalber streamen.
        false
    }

    /**
     * Löst eine ganze Liste auf und rechnet den Startpunkt um. Läuft im
     * Hintergrund. Gemeinsam für "Liste ersetzen" (eigene Oberfläche und
     * fremde Controller) und das Fortsetzen nach Neustart.
     *
     * WARUM den Index umrechnen? Nicht abspielbare Titel fallen heraus. Der
     * Startindex muss mitwandern – sonst zeigte er auf den falschen Titel
     * oder hinter das Listenende (IllegalSeekPositionException im Player).
     *
     * @throws IllegalArgumentException wenn GAR NICHTS abspielbar ist (Media3
     *         verwirft den Befehl dann, die laufende Warteschlange bleibt).
     */
    private fun resolveQueue(
        items: List<MediaItem>,
        startIndex: Int,
        startPositionMs: Long,
    ): MediaSession.MediaItemsWithStartPosition {
        val resolved = items.map(::resolve)
        val kept = resolved.filterNotNull()
        if (kept.isEmpty()) {
            throw IllegalArgumentException("Keiner der Titel ist abspielbar")
        }
        if (startIndex == C.INDEX_UNSET) {
            return MediaSession.MediaItemsWithStartPosition(kept, C.INDEX_UNSET, startPositionMs)
        }
        // Neuer Index = Zahl der behaltenen Titel VOR dem Start.
        // Fiel der Starttitel selbst weg, beginnt der nächste
        // abspielbare – dann von vorn.
        val start = startIndex.coerceIn(0, resolved.lastIndex)
        val newIndex = resolved.subList(0, start).count { it != null }
            .coerceAtMost(kept.lastIndex)
        val position = if (resolved[start] != null) startPositionMs else 0L
        return MediaSession.MediaItemsWithStartPosition(kept, newIndex, position)
    }

    // -----------------------------------------------------------------------
    // Abspielen für fremde Controller (Android Auto, Assistent, Widget)
    // -----------------------------------------------------------------------

    /**
     * Stammt das Item aus der eigenen Flutter-Oberfläche?
     *
     * Eigene Items tragen IMMER den Schlüssel [EXTRA_SONG_ID] (siehe
     * MediaItems.fromQueueEntry). Android Auto, Assistenten und Widgets
     * schicken dagegen nur eine Media-ID aus dem Inhaltsbaum ("album:…")
     * oder einen Suchbegriff – dafür baut Rust die Warteschlange.
     */
    private fun isOwnItem(item: MediaItem): Boolean =
        item.requestMetadata.extras?.containsKey(EXTRA_SONG_ID) == true

    /**
     * Baut in Rust die Warteschlange für eine fremde Anfrage. BLOCKIERT (Netz).
     *  - mit Media-ID: genau dieses Album/diese Playlist/diesen Titel,
     *  - ohne Media-ID mit Suchbegriff ("Spiele X auf Nocturne"): erster
     *    abspielbarer Treffer der Suche,
     *  - ganz ohne ("Spiele Musik auf Nocturne"): die zuletzt gespeicherte
     *    Warteschlange.
     *
     * @return `null`, wenn nichts gefunden wurde.
     */
    private fun foreignQueue(item: MediaItem): MediaItems.Queue? {
        NativeCore.ensureInit(this)
        val mediaId = item.mediaId.takeIf { it.isNotEmpty() }
        val json = if (mediaId != null) {
            NativeCore.playFromMediaId(mediaId)
        } else {
            val query = item.requestMetadata.searchQuery?.toString()?.trim().orEmpty()
            if (query.isEmpty()) {
                NativeCore.resume()
            } else {
                MediaItems.parseBrowseEntries(NativeCore.search(query))
                    .firstOrNull { it.playable }
                    ?.let { NativeCore.playFromMediaId(it.id) }
            }
        }
        return MediaItems.parseQueueJson(this, json)
    }

    /** Wie [foreignQueue], aber gleich aufgelöst (mit Stream-URLs). */
    private fun resolveForeign(item: MediaItem): MediaSession.MediaItemsWithStartPosition {
        val queue = foreignQueue(item)
            ?: throw IllegalArgumentException("Nichts gefunden für '${item.mediaId}'")
        return resolveQueue(queue.items, queue.index, queue.positionMs)
    }

    /** Gespeicherte Warteschlange aus Rust, aufgelöst. BLOCKIERT. */
    private fun resolveResume(): MediaSession.MediaItemsWithStartPosition {
        NativeCore.ensureInit(this)
        val queue = MediaItems.parseQueueJson(this, NativeCore.resume())
            ?: throw UnsupportedOperationException("Keine gespeicherte Warteschlange")
        return resolveQueue(queue.items, queue.index, queue.positionMs)
    }

    /**
     * Baut im Hintergrund eine Warteschlange und spielt sie dann direkt im
     * Player ab (für Widget-Knöpfe; die laufen nicht über die Session).
     */
    private fun loadAndPlay(build: () -> MediaSession.MediaItemsWithStartPosition) {
        val future = net.submit<MediaSession.MediaItemsWithStartPosition> { build() }
        Futures.addCallback(
            future,
            object : FutureCallback<MediaSession.MediaItemsWithStartPosition> {
                override fun onSuccess(result: MediaSession.MediaItemsWithStartPosition) {
                    // Dienst inzwischen beendet? Dann ist der Player freigegeben.
                    if (session == null) return
                    player.setMediaItems(result.mediaItems, result.startIndex, result.startPositionMs)
                    player.prepare()
                    player.play()
                }

                override fun onFailure(t: Throwable) {
                    Log.i(TAG, "Nichts abzuspielen: ${t.message}")
                }
            },
            ContextCompat.getMainExecutor(this),
        )
    }

    /**
     * Befehl eines Homescreen-Widgets (siehe WidgetActionReceiver). Läuft auf
     * dem Haupt-Thread, direkt am Player – der Empfänger ruft das nur auf,
     * wenn der Dienst in diesem Prozess schon läuft.
     */
    fun handleWidgetCommand(action: String, mediaId: String?) {
        when (action) {
            HomeWidgets.ACTION_PLAY_PAUSE -> {
                val active = player.playWhenReady &&
                    player.playbackState != Player.STATE_ENDED &&
                    player.playbackState != Player.STATE_IDLE
                when {
                    active -> player.pause()
                    // Leere Liste: wie Media3 bei "Play" ohne Titel -> fortsetzen.
                    player.mediaItemCount == 0 -> loadAndPlay(::resolveResume)
                    // Kümmert sich auch um IDLE (prepare) und ENDED (von vorn).
                    else -> Util.handlePlayButtonAction(player)
                }
            }
            HomeWidgets.ACTION_NEXT -> if (player.mediaItemCount > 0) player.seekToNext()
            HomeWidgets.ACTION_PREVIOUS -> if (player.mediaItemCount > 0) player.seekToPrevious()
            HomeWidgets.ACTION_PLAY_ID -> {
                val id = mediaId?.takeIf { it.isNotEmpty() } ?: return
                loadAndPlay { resolveForeign(MediaItem.Builder().setMediaId(id).build()) }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Hörzeit (Verlauf + Scrobbling)
    // -----------------------------------------------------------------------
    //
    // WARUM hier und nicht in Flutter? Die Oberfläche kann beendet sein,
    // während die Musik weiterläuft (Android räumt sie im Hintergrund ab,
    // oder es wurde nur über Android Auto/Widget gestartet). Dann fehlten
    // die Titel in Verlauf und Scrobbling. Der Dienst lebt dagegen genau so
    // lange wie die Wiedergabe.
    //
    // AKKU: Kein Timer. Gezählt wird über Differenzen von
    // SystemClock.elapsedRealtime() bei Zustandswechseln (Play/Pause,
    // Titelwechsel). Alles läuft auf dem Haupt-Thread (Player-Ereignisse).

    /** Eintrag, dessen Hörzeit gerade gezählt wird (= mediaItem.mediaId). */
    private var statEntryId: String? = null

    /** Bisher gehörte Zeit des Eintrags (ohne die laufende Strecke). */
    private var statListenedMs = 0L

    /** Seit wann läuft der Ton (elapsedRealtime); 0 = läuft nicht. */
    private var statPlayingSince = 0L

    /** Länge des Eintrags, sobald bekannt; 0 = unbekannt. */
    private var statDurationMs = 0L

    /** "Läuft gerade" schon für diesen Eintrag gemeldet? */
    private var statNowPlayingSent = false

    /** Beginnt die Zählung für einen neuen Eintrag. */
    private fun statsStart(item: MediaItem?) {
        statEntryId = item?.mediaId?.takeIf { it.isNotEmpty() }
        statListenedMs = 0L
        statPlayingSince = 0L
        statDurationMs = 0L
        statNowPlayingSent = false
        if (player.isPlaying) statsResume()
    }

    /** Ton läuft (wieder). */
    private fun statsResume() {
        val id = statEntryId ?: return
        if (statPlayingSince == 0L) statPlayingSince = SystemClock.elapsedRealtime()
        statsNoteDuration()
        // "Läuft gerade" erst, wenn wirklich Ton kommt – nicht schon, wenn
        // z. B. das Fortsetzen nach Neustart nur eine Liste vorbereitet.
        // Rust nutzt die Meldung zugleich als "aktueller Eintrag" fürs
        // Fortsetzen (siehe savePosition).
        if (!statNowPlayingSent) {
            statNowPlayingSent = true
            val app = applicationContext
            net.execute {
                NativeCore.ensureInit(app)
                NativeCore.nowPlaying(id)
            }
        }
    }

    /** Ton steht (Pause, Puffern, Fehler): laufende Strecke aufaddieren. */
    private fun statsPause() {
        if (statPlayingSince != 0L) {
            statListenedMs += SystemClock.elapsedRealtime() - statPlayingSince
            statPlayingSince = 0L
        }
        statsNoteDuration()
    }

    /**
     * Merkt sich die Länge, solange der Eintrag noch der aktuelle ist. Beim
     * Titelwechsel liefert player.duration schon die Länge des NEUEN Titels.
     */
    private fun statsNoteDuration() {
        if (player.currentMediaItem?.mediaId != statEntryId) return
        val d = player.duration
        if (d != C.TIME_UNSET && d > 0) statDurationMs = d
    }

    /**
     * Schließt den Eintrag ab. Gemeldet wird, wenn mindestens die Hälfte
     * ODER 4 Minuten gehört wurden (dieselbe Regel wie Last.fm; vorher in
     * Dart). Die Meldung läuft im Hintergrund, weil Rust dabei ins Netz geht.
     */
    private fun statsFinish() {
        statsPause()
        val id = statEntryId ?: return
        val listened = statListenedMs
        val duration = statDurationMs
        statEntryId = null
        statListenedMs = 0L
        statDurationMs = 0L
        val enough = listened >= REPORT_MIN_MS || (duration > 0 && listened * 2 >= duration)
        if (listened <= 0 || !enough) return
        val app = applicationContext
        net.execute {
            NativeCore.ensureInit(app)
            NativeCore.reportPlay(id, listened)
            // Der Verlauf hat sich geändert -> Schnellzugriff-Widget
            // (zuletzt gehört) nachziehen, falls eines existiert.
            HomeWidgets.refreshQuickAccess(app)
        }
    }

    // -----------------------------------------------------------------------
    // Homescreen-Widgets
    // -----------------------------------------------------------------------

    /** Zuletzt an die Widgets gemeldeter Zustand (ohne Position). */
    private var lastWidgetKey: String? = null

    /**
     * Meldet den Zustand an die Widgets – nur, wenn sich am BILD etwas
     * ändert (Titel, Cover, Play/Pause, Länge bekannt). Die Position des
     * Fortschrittsbalkens wird nur mitgenommen, nicht beobachtet: Der Balken
     * zeigt den Stand beim letzten Ereignis und läuft nicht live mit (Akku).
     *
     * @param serviceEnding Dienst endet -> Play-Symbol zeigen.
     */
    private fun pushWidgetState(force: Boolean = false, serviceEnding: Boolean = false) {
        val item = player.currentMediaItem
        val state = player.playbackState
        val playing = !serviceEnding && player.playWhenReady &&
            state != Player.STATE_ENDED && state != Player.STATE_IDLE
        val duration = player.duration.takeIf { it != C.TIME_UNSET && it > 0 } ?: 0L
        val np = HomeWidgets.NowPlaying(
            hasItem = item != null,
            title = item?.mediaMetadata?.title?.toString(),
            artist = item?.mediaMetadata?.artist?.toString(),
            coverId = MediaItems.coverIdOf(this, item),
            playing = playing,
            positionMs = player.currentPosition.coerceAtLeast(0L),
            durationMs = duration,
        )
        val key = "${np.hasItem}|${np.title}|${np.artist}|${np.coverId}|${np.playing}|${duration > 0}"
        if (!force && key == lastWidgetKey) return
        lastWidgetKey = key
        HomeWidgets.publish(this, np)
    }

    // -----------------------------------------------------------------------
    // Netzart an Rust melden (WLAN/ungemessen vs. mobil/gemessen)
    // -----------------------------------------------------------------------
    //
    // WARUM? Rust wählt danach Streaming-Qualität und ob z. B. Downloads/
    // Vorabladen erlaubt sind. Ohne laufende Oberfläche erfährt Rust das nur
    // von hier.
    //
    // Bewusst GETRENNT vom Rückruf "Fortsetzen nach Netzverlust" (unten):
    // Jener lebt nur während einer Fehlerserie und hat eigene Regeln. Beide
    // werden in onDestroy abgemeldet (kein Leck).

    private var networkTypeCallback: ConnectivityManager.NetworkCallback? = null

    /** Zuletzt an Rust gemeldete Netzart; -1 = noch nichts gemeldet. */
    private var reportedNetKind = -1

    /** Aktuelles Standard-Netz (für onLost: nur DAS zählt als "weg"). */
    private var currentDefaultNetwork: Network? = null

    /** System-Ereignis statt Polling: Android meldet jede Änderung selbst. */
    private fun watchNetworkType() {
        val cm = getSystemService(ConnectivityManager::class.java) ?: return
        // Einmal den aktuellen Stand melden – Rust soll nicht auf das erste
        // Ereignis warten müssen.
        val active = cm.activeNetwork
        currentDefaultNetwork = active
        reportNetworkKind(netKindOf(active?.let { cm.getNetworkCapabilities(it) }))
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                currentDefaultNetwork = network
                reportNetworkKind(netKindOf(caps))
            }

            override fun onLost(network: Network) {
                // Beim Wechsel WLAN -> mobil kann onLost(alt) NACH dem neuen
                // Netz kommen. Nur wenn das AKTUELLE Netz weg ist, gilt "kein Netz".
                if (network == currentDefaultNetwork) {
                    currentDefaultNetwork = null
                    reportNetworkKind(NativeCore.NET_NONE)
                }
            }
        }
        try {
            cm.registerDefaultNetworkCallback(cb, handler)
            networkTypeCallback = cb
        } catch (e: RuntimeException) {
            Log.w(TAG, "Netzart-Rückruf nicht möglich", e)
        }
    }

    private fun stopWatchingNetworkType() {
        val cb = networkTypeCallback ?: return
        networkTypeCallback = null
        try {
            getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(cb)
        } catch (e: IllegalArgumentException) {
            // War schon abgemeldet.
        }
    }

    private fun netKindOf(caps: NetworkCapabilities?): Int = when {
        caps == null || !caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) ->
            NativeCore.NET_NONE
        caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) ->
            NativeCore.NET_UNMETERED
        else -> NativeCore.NET_METERED
    }

    /** Nur echte Änderungen melden (Rückrufe kommen oft mehrfach). */
    private fun reportNetworkKind(kind: Int) {
        if (kind == reportedNetKind) return
        reportedNetKind = kind
        NativeCore.setNetwork(kind)
    }

    // -----------------------------------------------------------------------
    // Klangeffekte
    // -----------------------------------------------------------------------

    /**
     * Gleicht den Player mit dem Zustand der Rust-Effekte ab.
     *
     * AKKU: Sind ALLE Effekte aus, darf Android den Ton per Hardware-Offload
     * ausgeben. Dabei dekodiert der DSP-Chip des Handys, während die CPU
     * schläft – das spart bei langem Hören am meisten Strom.
     * Offload umgeht aber alle Audio-Prozessoren. Sobald ein Effekt an ist,
     * muss Offload deshalb aus sein, sonst wäre der Equalizer wirkungslos.
     *
     * Ebenso aus, solange ein Sleep-Timer mit Minuten läuft: Bei Offload
     * schläft die CPU, und Handler-Zeit (uptimeMillis) steht dann still –
     * der Timer würde viel zu spät auslösen. Ohne Offload hält
     * WAKE_MODE_NETWORK die CPU während der Wiedergabe wach.
     */
    private fun applyDspState() {
        // Auch Crossfade ist ein "Effekt": Der Ton muss dafür durch die
        // App laufen, statt direkt im Chip dekodiert zu werden.
        val effectsOn = NativeDsp.refreshActive() || crossfade.crossfadeMs > 0
        val offloadAllowed = !effectsOn && sleepDeadline == 0L
        val mode =
            if (offloadAllowed) TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_ENABLED
            else TrackSelectionParameters.AudioOffloadPreferences.AUDIO_OFFLOAD_MODE_DISABLED
        // Nur bei echter Änderung setzen: Neue TrackSelectionParameters lösen
        // eine Neuauswahl der Spuren aus.
        if (player.trackSelectionParameters.audioOffloadPreferences.audioOffloadMode == mode) return
        player.trackSelectionParameters = player.trackSelectionParameters
            .buildUpon()
            .setAudioOffloadPreferences(
                TrackSelectionParameters.AudioOffloadPreferences.Builder()
                    .setAudioOffloadMode(mode)
                    // Lückenlos ist uns wichtiger als Offload: Geräte, die
                    // Offload nur ohne Gapless können, bleiben bei normaler
                    // Wiedergabe.
                    .setIsGaplessSupportRequired(true)
                    .build(),
            )
            .build()
    }

    // -----------------------------------------------------------------------
    // Crossfade
    // -----------------------------------------------------------------------

    /** Überblendet Titelende und -anfang im Audio-Pfad (siehe Klasse). */
    private val crossfade = CrossfadeProcessor()

    private fun playbackPrefs() = getSharedPreferences(PREFS_PLAYBACK, Context.MODE_PRIVATE)

    /** Neue Crossfade-Dauer aus den Einstellungen; 0 = aus. Wirkt sofort. */
    private fun setCrossfade(seconds: Int) {
        val ms = seconds.coerceIn(0, CrossfadeBuffer.MAX_MS / 1000) * 1000
        if (ms == crossfade.crossfadeMs) return
        crossfade.crossfadeMs = ms
        playbackPrefs().edit().putInt(PREF_CROSSFADE_MS, ms).apply()
        // Hardware-Offload passend um- oder zurückschalten.
        applyDspState()
    }

    private fun audioManager() = getSystemService(Context.AUDIO_SERVICE) as AudioManager

    /** Android ruft das auf, sobald ein Ausgabegerät dazukommt oder wegfällt. */
    private val deviceCallback = object : AudioDeviceCallback() {
        override fun onAudioDevicesAdded(addedDevices: Array<out AudioDeviceInfo>?) =
            reportOutputDevice()

        override fun onAudioDevicesRemoved(removedDevices: Array<out AudioDeviceInfo>?) =
            reportOutputDevice()
    }

    /**
     * Beobachtet das Ausgabegerät für "EQ-Profil je Gerät".
     * Das ist ein System-Ereignis: Es wird nichts regelmäßig abgefragt.
     */
    private fun watchOutputDevice() {
        audioManager().registerAudioDeviceCallback(deviceCallback, handler)
        reportOutputDevice()
    }

    /**
     * Ermittelt, wo der Ton gerade herauskommt, und meldet es an Rust.
     *
     * Android sagt nicht direkt "aktives Gerät", sondern listet alle
     * angeschlossenen auf. Wir nehmen deshalb dieselbe Rangfolge, nach der
     * Android selbst Musik ausgibt: Bluetooth vor USB vor Kabel vor dem
     * eingebauten Lautsprecher.
     */
    private fun reportOutputDevice() {
        if (!NativeDsp.available) return
        val devices = audioManager().getDevices(AudioManager.GET_DEVICES_OUTPUTS)
        fun find(vararg types: Int) = devices.firstOrNull { it.type in types }

        val bluetooth = find(
            AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
            AudioDeviceInfo.TYPE_BLE_HEADSET,
            AudioDeviceInfo.TYPE_BLE_SPEAKER,
            AudioDeviceInfo.TYPE_HEARING_AID,
        )
        val usb = find(
            AudioDeviceInfo.TYPE_USB_HEADSET,
            AudioDeviceInfo.TYPE_USB_DEVICE,
            AudioDeviceInfo.TYPE_USB_ACCESSORY,
        )
        val wired = find(
            AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
            AudioDeviceInfo.TYPE_WIRED_HEADSET,
            AudioDeviceInfo.TYPE_LINE_ANALOG,
        )
        // Bluetooth- und USB-Geräte bekommen ihren Namen in den Schlüssel:
        // Zwei verschiedene Kopfhörer sollen zwei Profile haben können.
        val (key, name) = when {
            bluetooth != null -> bluetooth.productName.toString().let { "bt:$it" to it }
            usb != null -> usb.productName.toString().let { "usb:$it" to it }
            wired != null -> "wired" to "Kabel"
            else -> "speaker" to "Lautsprecher"
        }
        NativeDsp.nativeSetOutputDevice(key, name)
        // Das neue Gerät kann ein Profil mit ein- oder ausgeschaltetem EQ haben.
        applyDspState()
    }

    // -----------------------------------------------------------------------
    // Sleep-Timer
    // -----------------------------------------------------------------------

    /**
     * Marken für die Handler-Nachrichten des Sleep-Timers. Über sie lassen
     * sich beim Abbrechen alle auf einmal entfernen – auch eine bereits
     * laufende Ausblendung (deren Lautstärke- und Pause-Schritte sonst
     * weiterliefen und trotz Abbruch pausieren würden).
     *  - [sleepCheckToken]: der eine "Ist die Zeit um?"-Weckruf
     *  - [sleepFadeToken] : die Schritte der Ausblendung und die Pause
     */
    private val sleepCheckToken = Any()
    private val sleepFadeToken = Any()

    /**
     * Frist des Timers in `SystemClock.elapsedRealtime()`, 0 = kein Timer.
     * WARUM elapsedRealtime? Die läuft auch im Tiefschlaf weiter, die
     * Handler-Zeit (uptimeMillis) nicht. Der Handler weckt uns nur; ob die
     * Zeit WIRKLICH um ist, entscheidet diese Frist.
     */
    private var sleepDeadline = 0L

    /**
     * WARUM läuft der Timer hier im Dienst und nicht in Dart?
     * Die Flutter-UI kann jederzeit von Android beendet werden, während die
     * Musik weiterläuft. Ein Timer in Dart würde dann nie auslösen. Der
     * Dienst lebt dagegen genau so lange wie die Wiedergabe.
     *
     * @param minutes  > 0: nach so vielen Minuten stoppen
     *                 -1 : am Ende des aktuellen Titels stoppen
     *                  0 : Timer abbrechen
     */
    private fun setSleepTimer(minutes: Int) {
        cancelSleepTimer()
        when {
            minutes > 0 -> {
                val delay = minutes * 60_000L
                sleepDeadline = SystemClock.elapsedRealtime() + delay
                scheduleSleepCheck(delay)
            }
            minutes == -1 -> player.pauseAtEndOfMediaItems = true
        }
        // Offload hängt vom Timer ab (siehe applyDspState).
        applyDspState()
    }

    /**
     * Bricht Timer UND eine evtl. laufende Ausblendung ab. Die Lautstärke
     * geht auf 1 zurück – sonst bliebe die Musik nach dem Abbrechen mitten
     * im Ausblenden leise.
     */
    private fun cancelSleepTimer() {
        handler.removeCallbacksAndMessages(sleepCheckToken)
        handler.removeCallbacksAndMessages(sleepFadeToken)
        sleepDeadline = 0L
        sleepFading = false
        player.pauseAtEndOfMediaItems = false
        player.volume = 1f
    }

    /** `true`, solange die 4-s-Ausblendung läuft. */
    private var sleepFading = false

    private fun post(token: Any, delayMs: Long, block: () -> Unit) {
        // postAtTime mit Token gibt es seit API 1 (postDelayed mit Token erst ab 28).
        handler.postAtTime(block, token, SystemClock.uptimeMillis() + delayMs)
    }

    /** (Neu) planen, wann geprüft wird; ersetzt einen alten Weckruf. */
    private fun scheduleSleepCheck(delayMs: Long) {
        handler.removeCallbacksAndMessages(sleepCheckToken)
        post(sleepCheckToken, delayMs) { checkSleepTimer() }
    }

    /**
     * Die Wiedergabe soll (wieder) laufen. Zwei Fälle, beide Folge davon,
     * dass die Handler-Zeit im Tiefschlaf (pausiert, CPU aus) stillsteht:
     *  - Frist schon abgelaufen: Der Timer hat sein Ziel ("Musik aus")
     *    erreicht, als ohnehin nichts lief. Das neue Abspielen NICHT sofort
     *    wieder ausblenden, sondern den Timer still beenden.
     *  - Frist läuft noch: Der alte Weckruf käme um die verschlafene Zeit zu
     *    spät -> anhand von elapsedRealtime neu planen.
     */
    private fun onPlaybackResumedForSleepTimer() {
        if (sleepDeadline == 0L || sleepFading) return
        val remaining = sleepDeadline - SystemClock.elapsedRealtime()
        if (remaining <= 0) {
            cancelSleepTimer()
            applyDspState()
        } else {
            scheduleSleepCheck(remaining)
        }
    }

    /** Der Handler hat geweckt: Ist die Frist wirklich um? */
    private fun checkSleepTimer() {
        if (sleepDeadline == 0L) return
        val remaining = sleepDeadline - SystemClock.elapsedRealtime()
        when {
            // Zu früh geweckt (sollte nicht vorkommen) -> Rest abwarten.
            remaining > 0 -> scheduleSleepCheck(remaining)
            // playWhenReady statt isPlaying: Auch während kurzen Pufferns
            // will der Nutzer Musik -> trotzdem ausblenden und pausieren.
            player.playWhenReady -> fadeOutAndPause()
            else -> {
                // Läuft ohnehin nichts: Ziel erreicht, nur aufräumen.
                cancelSleepTimer()
                applyDspState()
            }
        }
    }

    /** Blendet über einige Sekunden aus, statt hart abzuschneiden. */
    private fun fadeOutAndPause() {
        sleepFading = true
        val steps = 20
        val stepMs = FADE_MS / steps
        for (i in 1..steps) {
            post(sleepFadeToken, i * stepMs) { player.volume = 1f - i / steps.toFloat() }
        }
        post(sleepFadeToken, FADE_MS + 50) {
            player.pause()
            // Lautstärke zurücksetzen, sonst wäre der nächste Start stumm.
            cancelSleepTimer()
            applyDspState()
        }
    }

    // -----------------------------------------------------------------------
    // Fortsetzen nach Netzverlust
    // -----------------------------------------------------------------------

    private var networkCallback: ConnectivityManager.NetworkCallback? = null

    /**
     * Pro Fehlerserie darf EIN Neuversuch im selben Netz passieren (z. B.
     * WLAN -> Mobilfunk war schon umgeschaltet, als der Fehler kam). Danach
     * nur noch, wenn sich das Netz wirklich ändert. So entsteht keine
     * Endlosschleife, wenn das Netz da ist, aber der Server nicht antwortet.
     * Zurückgesetzt, sobald wieder gespielt wird (STATE_READY).
     */
    private var retriedOnSameNetwork = false

    /**
     * Die Wiedergabe ist wegen eines Netzwerkfehlers stehen geblieben.
     *
     * AKKU: Kein Polling. Wir melden uns NUR für diese Zeit beim System an
     * und lassen uns wecken, wenn sich das Standard-Netz ändert. Danach wird
     * der Rückruf sofort wieder abgemeldet.
     */
    private fun waitForNetwork() {
        if (networkCallback != null) return
        val cm = getSystemService(ConnectivityManager::class.java) ?: return
        val networkAtError: Network? = cm.activeNetwork
        var sawLoss = false
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onLost(network: Network) {
                sawLoss = true
            }

            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                // Nur "hat Internet" prüfen, nicht "VALIDATED": Ein Server im
                // Heimnetz ohne Internetzugang soll auch zählen.
                if (!caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)) return
                val changed = sawLoss || network != networkAtError
                if (!changed) {
                    if (retriedOnSameNetwork) return
                    retriedOnSameNetwork = true
                }
                resumeAfterNetwork()
            }
        }
        try {
            // Mit Handler: Rückrufe kommen auf dem Haupt-Thread, wie alle
            // anderen Player-Aufrufe (API 26+).
            cm.registerDefaultNetworkCallback(cb, handler)
            networkCallback = cb
        } catch (e: RuntimeException) {
            // z. B. zu viele Rückrufe registriert: dann eben kein Auto-Fortsetzen.
            Log.w(TAG, "Netz-Rückruf nicht möglich", e)
        }
    }

    private fun stopWaitingForNetwork() {
        val cb = networkCallback ?: return
        networkCallback = null
        try {
            getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(cb)
        } catch (e: IllegalArgumentException) {
            // War schon abgemeldet.
        }
    }

    private fun resumeAfterNetwork() {
        stopWaitingForNetwork()
        // Nur fortsetzen, wenn noch derselbe Fehlerzustand vorliegt und der
        // Nutzer nicht inzwischen pausiert hat.
        if (player.playbackState == Player.STATE_IDLE && player.playerError != null &&
            player.playWhenReady
        ) {
            player.prepare()
        }
    }

    // -----------------------------------------------------------------------
    // Session-Callback
    // -----------------------------------------------------------------------

    private inner class LibraryCallback : MediaLibrarySession.Callback {

        /**
         * Entscheidet, wer sich mit der Session verbinden darf, und erlaubt
         * den Zugelassenen zusätzlich unsere eigenen Befehle (Sleep-Timer,
         * Klang geändert). Ohne diesen Eintrag lehnt Media3 sie ab.
         */
        override fun onConnect(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
        ): MediaSession.ConnectionResult {
            if (!isAllowedController(session, controller)) {
                Log.i(TAG, "Controller abgelehnt: ${controller.packageName}")
                return MediaSession.ConnectionResult.reject()
            }
            val commands = MediaSession.ConnectionResult.DEFAULT_SESSION_AND_LIBRARY_COMMANDS
                .buildUpon()
                .add(SessionCommand(CMD_SLEEP_TIMER, Bundle.EMPTY))
                .add(SessionCommand(CMD_DSP_CHANGED, Bundle.EMPTY))
                .add(SessionCommand(CMD_CROSSFADE, Bundle.EMPTY))
                .build()
            return MediaSession.ConnectionResult.AcceptedResultBuilder(session)
                .setAvailableSessionCommands(commands)
                .build()
        }

        /**
         * Zugelassen sind:
         *  - die eigene App (gleiche UID: Oberfläche, Benachrichtigung,
         *    Media-Tasten-Empfänger),
         *  - "vertrauenswürdige" Controller (Media3: System-Apps bzw. Apps mit
         *    MEDIA_CONTENT_CONTROL oder aktivem Benachrichtigungs-Zugriff),
         *  - Benachrichtigung, Android Automotive und Android Auto (Media3
         *    erkennt sie selbst),
         *  - eine kleine Liste bekannter System-/Google-Pakete.
         *
         * GRENZE DIESER LÖSUNG: Die Liste prüft nur den PAKETNAMEN, nicht die
         * Signatur. Android garantiert, dass ein Paketname auf dem Gerät nur
         * einmal vergeben ist; eine fremde App könnte sich aber nur dann als
         * "com.google.android.projection.gearhead" ausgeben, wenn das echte
         * Paket fehlt. Für eine echte Prüfung bräuchte es eine Liste der
         * Signatur-Hashes (wie Googles "PackageValidator"-Beispiel). Weil die
         * Session nach dieser Änderung keine Anmeldedaten mehr enthält, wäre
         * der Schaden eines Fälschers auf "Musik steuern / Titel sehen"
         * begrenzt.
         */
        private fun isAllowedController(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
        ): Boolean {
            if (controller.uid == Process.myUid()) return true
            if (controller.isTrusted) return true
            if (session.isMediaNotificationController(controller) ||
                session.isAutomotiveController(controller) ||
                session.isAutoCompanionController(controller)
            ) return true
            if (controller.packageName in ALLOWED_PACKAGES) return true
            // Android 8/8.1: Das System verrät bei Aufrufen über die alte
            // MediaSession-Schnittstelle (Sperrbildschirm, Bluetooth-Tasten)
            // nicht, wer ruft – alle kommen als "LEGACY_CONTROLLER". Lehnten wir
            // sie ab, ginge auf diesen Geräten die Systemsteuerung nicht mehr.
            if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P &&
                controller.packageName == MediaSession.ControllerInfo.LEGACY_CONTROLLER_PACKAGE_NAME
            ) return true
            return false
        }

        override fun onCustomCommand(
            session: MediaSession,
            controller: MediaSession.ControllerInfo,
            customCommand: SessionCommand,
            args: Bundle,
        ): ListenableFuture<SessionResult> {
            if (customCommand.customAction == CMD_SLEEP_TIMER) {
                setSleepTimer(args.getInt(ARG_MINUTES, 0))
                return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
            }
            if (customCommand.customAction == CMD_CROSSFADE) {
                setCrossfade(args.getInt(ARG_SECONDS, 0))
                return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
            }
            if (customCommand.customAction == CMD_DSP_CHANGED) {
                // Die Oberfläche hat Klang-Einstellungen geändert.
                applyDspState()
                return Futures.immediateFuture(SessionResult(SessionResult.RESULT_SUCCESS))
            }
            return super.onCustomCommand(session, controller, customCommand, args)
        }

        /**
         * Ergänzt bei jedem Titel die Abspiel-URL.
         *
         * WARUM nötig? Aus Sicherheitsgründen überträgt Media3 die URL eines
         * MediaItems NICHT vom Controller zum Dienst. Und wir schicken sie
         * auch bewusst nicht in `requestMetadata` mit (Token!). Der Dienst
         * baut sie deshalb hier aus der Song-ID (siehe [resolve]).
         *
         * Läuft auf dem Hintergrund-Thread; Media3 wartet auf das Future.
         * Nicht auflösbare Items werden herausgefiltert, statt den Dienst
         * abstürzen zu lassen.
         *
         * FREMDE Items (ohne songId: Android Auto, Assistent) beschreiben
         * keinen einzelnen Titel, sondern "spiele Album X". Rust ersetzt dafür
         * seine Warteschlange (nativePlayFromMediaId). Damit Player und Rust
         * nicht auseinanderlaufen, wird aus "Hinzufügen" dann ebenfalls
         * "Ersetzen": Wir setzen die neue Liste direkt im Player und geben
         * Media3 eine leere Liste zurück (= nichts zusätzlich anhängen).
         */
        override fun onAddMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
        ): ListenableFuture<MutableList<MediaItem>> {
            val foreign = mediaItems.firstOrNull { !isOwnItem(it) }
                ?: return background.submit<MutableList<MediaItem>> {
                    // Eine leere Liste ist hier harmlos: "nichts hinzufügen".
                    mediaItems.mapNotNull(::resolve).toMutableList()
                }
            return net.submit<MutableList<MediaItem>> {
                val result = resolveForeign(foreign)
                handler.post {
                    if (session == null) return@post
                    player.setMediaItems(result.mediaItems, result.startIndex, result.startPositionMs)
                    player.prepare()
                }
                mutableListOf()
            }
        }

        /**
         * Wie [onAddMediaItems], aber für "Liste ersetzen" (siehe
         * [resolveQueue] zum Umrechnen des Startindex).
         *
         * Ist GAR NICHTS abspielbar, schlägt das Future fehl: Eine leere Liste
         * würde die laufende Warteschlange löschen, nur weil z. B. ein
         * Assistent etwas Unbekanntes wollte. Media3 verwirft den Befehl dann.
         *
         * Fremde Items (Android Auto "Album antippen", Assistent "Spiele X",
         * Schnellzugriff-Widget) laufen über [resolveForeign]: Rust baut die
         * Warteschlange, Startindex und Position kommen von dort.
         */
        override fun onSetMediaItems(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
            mediaItems: MutableList<MediaItem>,
            startIndex: Int,
            startPositionMs: Long,
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> {
            if (mediaItems.isEmpty()) {
                // Wirklich leer gewollt (Liste leeren) -> durchreichen.
                return Futures.immediateFuture(
                    MediaSession.MediaItemsWithStartPosition(emptyList(), startIndex, startPositionMs),
                )
            }
            if (mediaItems.all(::isOwnItem)) {
                return background.submit<MediaSession.MediaItemsWithStartPosition> {
                    resolveQueue(mediaItems, startIndex, startPositionMs)
                }
            }
            // Bevorzugt das Item am Startindex (das hat der Nutzer gewählt).
            val foreign = mediaItems.getOrNull(startIndex)?.takeIf { !isOwnItem(it) }
                ?: mediaItems.first { !isOwnItem(it) }
            return net.submit<MediaSession.MediaItemsWithStartPosition> { resolveForeign(foreign) }
        }

        /**
         * "Weiterspielen" nach Neustart: Systemoberfläche (Medienkarte),
         * Bluetooth-Play-Taste bei beendetem Dienst, Play-Knopf eines Widgets
         * bei leerer Liste. Rust liefert die zuletzt gespeicherte Warteschlange
         * samt Position (siehe [savePosition]). Ohne gespeicherte Liste schlägt
         * das Future fehl – das ist das vorgesehene Standardverhalten.
         */
        override fun onPlaybackResumption(
            mediaSession: MediaSession,
            controller: MediaSession.ControllerInfo,
        ): ListenableFuture<MediaSession.MediaItemsWithStartPosition> =
            net.submit<MediaSession.MediaItemsWithStartPosition> { resolveResume() }

        /**
         * Der "Bibliotheks-Baum" für Android Auto, Wear OS und Assistenten.
         *
         * Die Wurzel selbst ist fest ("root"); ihre Kinder (zuletzt gehört,
         * Playlists, Alben, Downloads) und alles darunter liefert Rust über
         * nativeBrowse. Die "Recent"-Wurzel der Systemoberfläche (Media
         * Resumption) beantwortet Media3 selbst über [onPlaybackResumption].
         *
         * Die Extras sagen Android Auto: Suche wird unterstützt, Ordner/Alben
         * als Raster, Titel als Liste.
         */
        override fun onGetLibraryRoot(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<MediaItem>> {
            val root = MediaItem.Builder()
                .setMediaId(ROOT_ID)
                .setMediaMetadata(
                    MediaMetadata.Builder()
                        .setTitle("Nocturne")
                        .setIsBrowsable(true)
                        .setIsPlayable(false)
                        .setMediaType(MediaMetadata.MEDIA_TYPE_FOLDER_MIXED)
                        .build(),
                )
                .build()
            val rootParams = LibraryParams.Builder()
                .setOffline(params?.isOffline ?: false)
                .setRecent(params?.isRecent ?: false)
                .setSuggested(params?.isSuggested ?: false)
                .setExtras(Bundle().apply {
                    putBoolean(AUTO_SEARCH_SUPPORTED, true)
                    putInt(AUTO_BROWSABLE_HINT, AUTO_STYLE_GRID)
                    putInt(AUTO_PLAYABLE_HINT, AUTO_STYLE_LIST)
                })
                .build()
            return Futures.immediateFuture(LibraryResult.ofItem(root, rootParams))
        }

        /** Kinder eines Ordners – über Rust, im Hintergrund (Netz möglich). */
        override fun onGetChildren(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
            net.submit<LibraryResult<ImmutableList<MediaItem>>> {
                NativeCore.ensureInit(this@PlaybackService)
                val items = MediaItems.parseBrowseJson(
                    this@PlaybackService, NativeCore.browse(parentId, german),
                )
                remember(items)
                LibraryResult.ofItemList(pageOf(items, page, pageSize), params)
            }

        /**
         * Einzelnes Item. Rust hat dafür (noch) keine eigene Funktion; wir
         * kennen aber alles, was wir zuvor in Ordnern/Suchergebnissen
         * ausgeliefert haben ([knownItems]). Das genügt Android Auto, das nur
         * nach Items fragt, die es vorher gesehen hat.
         */
        override fun onGetItem(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            mediaId: String,
        ): ListenableFuture<LibraryResult<MediaItem>> {
            if (mediaId == ROOT_ID) {
                return onGetLibraryRoot(session, browser, null)
            }
            val item = synchronized(knownItems) { knownItems[mediaId] }
                ?: return Futures.immediateFuture(LibraryResult.ofError<MediaItem>(SessionError.ERROR_BAD_VALUE))
            return Futures.immediateFuture(LibraryResult.ofItem(item, null))
        }

        /**
         * Abonnieren immer erlauben. Die Standard-Umsetzung fragt erst
         * [onGetItem] – das würde bei Ordnern scheitern, die wir (noch) nicht
         * kennen, und manche Controller zeigen dann nichts an. Wir melden
         * ohnehin keine Änderungen am Baum (notifyChildrenChanged), weil der
         * Inhalt beim nächsten Öffnen frisch aus Rust kommt.
         */
        override fun onSubscribe(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            parentId: String,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<Void>> =
            Futures.immediateFuture(LibraryResult.ofVoid())

        /**
         * Suche (Android Auto Suchfeld, Assistent). Media3-Ablauf: erst
         * onSearch -> wir suchen im Hintergrund und melden die Trefferzahl mit
         * notifySearchResultChanged; dann holt der Controller die Treffer mit
         * onGetSearchResult ab.
         */
        override fun onSearch(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<Void>> {
            net.execute {
                val count = search(query).size
                handler.post {
                    this@PlaybackService.session
                        ?.notifySearchResultChanged(browser, query, count, params)
                }
            }
            return Futures.immediateFuture(LibraryResult.ofVoid())
        }

        override fun onGetSearchResult(
            session: MediaLibrarySession,
            browser: MediaSession.ControllerInfo,
            query: String,
            page: Int,
            pageSize: Int,
            params: LibraryParams?,
        ): ListenableFuture<LibraryResult<ImmutableList<MediaItem>>> =
            net.submit<LibraryResult<ImmutableList<MediaItem>>> {
                // Manche Controller rufen direkt hier an, ohne onSearch.
                val items = synchronized(searchResults) { searchResults[query] } ?: search(query)
                LibraryResult.ofItemList(pageOf(items, page, pageSize), params)
            }
    }

    // -----------------------------------------------------------------------
    // Hilfen für den Inhaltsbaum
    // -----------------------------------------------------------------------

    /**
     * Alles, was wir zuletzt an Controller ausgeliefert haben (für onGetItem).
     * Begrenzt (LRU), damit langes Stöbern im Auto keinen Speicher frisst.
     */
    private val knownItems = object : LinkedHashMap<String, MediaItem>(64, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, MediaItem>?) =
            size > MAX_KNOWN_ITEMS
    }

    /** Ergebnisse der letzten Suchanfragen (Begriff -> Treffer), begrenzt. */
    private val searchResults = object : LinkedHashMap<String, List<MediaItem>>(8, 0.75f, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<String, List<MediaItem>>?) =
            size > MAX_SEARCHES
    }

    private fun remember(items: List<MediaItem>) {
        synchronized(knownItems) { items.forEach { knownItems[it.mediaId] = it } }
    }

    /** Sucht über Rust und merkt sich das Ergebnis. BLOCKIERT. */
    private fun search(query: String): List<MediaItem> {
        NativeCore.ensureInit(this)
        val items = MediaItems.parseBrowseJson(this, NativeCore.search(query))
        remember(items)
        synchronized(searchResults) { searchResults[query] = items }
        return items
    }

    /** Eine Seite aus der Liste (Media3 blättert: page ab 0, pageSize). */
    private fun pageOf(items: List<MediaItem>, page: Int, pageSize: Int): List<MediaItem> {
        if (pageSize <= 0 || page < 0) return items
        val from = page.toLong() * pageSize
        if (from >= items.size) return emptyList()
        val to = minOf(items.size.toLong(), from + pageSize)
        return items.subList(from.toInt(), to.toInt())
    }

    companion object {
        private const val TAG = "PlaybackService"
        private const val ROOT_ID = "root"
        private const val FADE_MS = 4000L

        /** Ab so viel gehörter Zeit zählt ein Titel immer (auch wenn er lang ist). */
        private const val REPORT_MIN_MS = 4 * 60_000L

        private const val MAX_KNOWN_ITEMS = 500
        private const val MAX_SEARCHES = 5

        // Schlüssel, die Android Auto in den Wurzel-Extras auswertet
        // (androidx.media.utils.MediaConstants; als Text, um keine weitere
        // Bibliothek einzubinden).
        private const val AUTO_SEARCH_SUPPORTED = "android.media.browse.SEARCH_SUPPORTED"
        private const val AUTO_BROWSABLE_HINT = "android.media.browse.CONTENT_STYLE_BROWSABLE_HINT"
        private const val AUTO_PLAYABLE_HINT = "android.media.browse.CONTENT_STYLE_PLAYABLE_HINT"
        private const val AUTO_STYLE_LIST = 1
        private const val AUTO_STYLE_GRID = 2

        /**
         * Der laufende Dienst in DIESEM Prozess (oder null). Nur für den
         * WidgetActionReceiver: Läuft der Dienst schon, gehen Widget-Knöpfe
         * direkt an den Player, statt eine Verbindung aufzubauen.
         * WeakReference: Ein statisches Feld darf den Dienst nicht am Leben
         * halten (Speicherleck nach onDestroy).
         */
        @Volatile
        private var instance: WeakReference<PlaybackService>? = null

        fun running(): PlaybackService? = instance?.get()

        /**
         * Ist gerade eine Flutter-Oberfläche verbunden, die Ereignisse
         * empfängt? Gesetzt von der PlayerBridge. Solange ja, führt Dart die
         * Warteschlange in Rust nach; sonst der Dienst selbst (siehe
         * syncQueueWithoutUi).
         */
        @Volatile
        var uiAttached = false

        /**
         * Fehler, nach denen ein Neuversuch bei wiederkehrendem Netz Sinn hat.
         * NICHT dabei: falscher Content-Type (Server-Fehlermeldung, siehe
         * AudioOnlyDataSource) oder HTTP-Fehlerstatus – das löst ein neues
         * Netz nicht.
         */
        private val NETWORK_ERRORS = setOf(
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_FAILED,
            PlaybackException.ERROR_CODE_IO_NETWORK_CONNECTION_TIMEOUT,
        )

        /**
         * Bekannte System-/Google-Pakete, die die Session steuern dürfen
         * (siehe isAllowedController, inkl. Grenze der Prüfung).
         */
        private val ALLOWED_PACKAGES = setOf(
            "com.google.android.projection.gearhead", // Android Auto
            "com.google.android.wearable.app", // Wear OS
            "com.google.android.googlequicksearchbox", // Google Assistant
            "com.google.android.carassistant", // Assistant im Auto
            "com.android.systemui", // Medien-Steuerung, Schnelleinstellungen
            "com.android.bluetooth", // Bluetooth-Tasten (AVRCP)
        )

        /** Eigener Session-Befehl für den Sleep-Timer. */
        const val CMD_SLEEP_TIMER = "musicplayer.SLEEP_TIMER"
        const val ARG_MINUTES = "minutes"

        /** Die Oberfläche meldet: Klang-Einstellungen wurden geändert. */
        const val CMD_DSP_CHANGED = "musicplayer.DSP_CHANGED"

        /** Crossfade-Dauer setzen; [ARG_SECONDS] = Sekunden, 0 = aus. */
        const val CMD_CROSSFADE = "musicplayer.CROSSFADE"
        const val ARG_SECONDS = "seconds"

        private const val PREFS_PLAYBACK = "playback"
        private const val PREF_CROSSFADE_MS = "crossfadeMs"

        /** Schlüssel, unter dem die Song-ID im MediaItem mitreist. */
        const val EXTRA_SONG_ID = "songId"

        /** ReplayGain-Werte des Titels (Float, fehlen wenn unbekannt). */
        const val EXTRA_TRACK_GAIN = "trackGain"
        const val EXTRA_ALBUM_GAIN = "albumGain"
        const val EXTRA_TRACK_PEAK = "trackPeak"
        const val EXTRA_ALBUM_PEAK = "albumPeak"
    }
}
