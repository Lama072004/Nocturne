package io.github.lama072004.nocturne

import android.content.Context
import android.util.Log
import com.google.common.util.concurrent.ListeningExecutorService
import com.google.common.util.concurrent.MoreExecutors
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

/**
 * Kotlin-Seite der Kern-Funktionen von Rust, die NICHTS mit dem Klang zu tun
 * haben: Einrichten des Kerns, das Bauen von Server-Adressen und – damit die
 * App auch OHNE Flutter-Oberfläche eine vollständige Musik-App ist – der
 * Inhaltsbaum, das Starten/Fortsetzen von Warteschlangen und die Hörstatistik.
 *
 * WARUM baut Rust die Adressen und nicht Dart?
 * Stream- und Cover-Adressen enthalten Anmelde-Token und Salt. Würde Dart sie
 * fertig in die MediaItems legen, könnte jede App, die sich mit unserer
 * MediaSession verbindet, sie auslesen. Deshalb reisen nur noch IDs durch die
 * Session; der Dienst (bzw. der ArtworkProvider) holt sich die Adresse erst
 * im letzten Moment hier – innerhalb unseres eigenen Prozesses.
 *
 * VERTRAG mit `rust/src/jni_bridge.rs` (Namen nicht ändern!):
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeInit(dataDir: String, storageKey: byte[])
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeStreamUrl(songId: String) -> String?
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeCoverUrl(coverId: String, size: int) -> String?
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeBrowse(parentId: String, german: boolean) -> String
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeSearch(query: String) -> String
 *   Java_io_github_lama072004_nocturne_NativeCore_nativePlayFromMediaId(mediaId: String) -> String?
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeResume() -> String?
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeSavePosition(positionMs: long)
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeSetNetwork(kind: int)
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeReportPlay(entryId: String, listenedMs: long)
 *   Java_io_github_lama072004_nocturne_NativeCore_nativeNowPlaying(entryId: String)
 *
 * Alle Aufrufe gehen über die kleinen Hüllen unten ([ensureInit],
 * [streamUrl], [coverUrl], [browse], …). Sie fangen `UnsatisfiedLinkError` ab:
 * Fehlt eine Funktion in der geladenen Bibliothek (alte Rust-Version), läuft
 * die App weiter, statt beim ersten Titel abzustürzen.
 *
 * THREADS: Alles außer [savePosition] und [setNetwork] kann blockieren (Netz)
 * und gehört auf [executor], NIE auf den Haupt-Thread.
 */
object NativeCore {
    private const val TAG = "NativeCore"

    /**
     * `false`, wenn die Rust-Bibliothek nicht geladen werden konnte.
     * Es ist dieselbe .so-Datei wie bei [NativeDsp]; Android lädt sie nur
     * einmal pro Prozess, ein zweites `loadLibrary` kostet nichts.
     */
    val available: Boolean = try {
        System.loadLibrary("rust_lib_musicplayer")
        true
    } catch (e: UnsatisfiedLinkError) {
        false
    }

    @Volatile
    private var initialized = false

    /**
     * Richtet den Rust-Kern ein, falls das in diesem Prozess noch nicht
     * passiert ist. Wird vom Dienst UND vom ArtworkProvider aufgerufen, weil
     * beide ohne laufende Flutter-UI starten können (Android Auto, fremde App
     * fragt ein Cover ab). Rust selbst ist ebenfalls idempotent; das Flag
     * hier spart nur den wiederholten Gang zum Schlüsselspeicher.
     *
     * @return `true`, wenn der Kern bereit ist.
     */
    @Synchronized
    fun ensureInit(context: Context): Boolean {
        if (initialized) return true
        if (!available) return false
        val app = context.applicationContext
        return try {
            nativeInit(app.filesDir.absolutePath, SecretStore.key(app))
            initialized = true
            true
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "nativeInit fehlt in der Rust-Bibliothek", e)
            false
        } catch (e: Exception) {
            // z. B. Schlüsselspeicher vorübergehend nicht erreichbar. Beim
            // nächsten Aufruf wird es erneut versucht.
            Log.w(TAG, "Rust-Kern konnte nicht eingerichtet werden", e)
            false
        }
    }

    /** Abspiel-Adresse eines Titels; `null` = nicht angemeldet / unbekannt. */
    fun streamUrl(songId: String): String? = guarded { nativeStreamUrl(songId) }

    /** Cover-Adresse in etwa [size] Pixeln; `null` = nicht angemeldet / unbekannt. */
    fun coverUrl(coverId: String, size: Int): String? = guarded { nativeCoverUrl(coverId, size) }

    private inline fun guarded(block: () -> String?): String? {
        if (!available) return null
        return try {
            block()?.takeIf { it.isNotBlank() }
        } catch (e: UnsatisfiedLinkError) {
            Log.w(TAG, "Rust-Funktion fehlt", e)
            null
        }
    }

    // -----------------------------------------------------------------------
    // Bibliothek, Wiedergabe-Start, Fortsetzen, Statistik
    // (für Android Auto, Assistent, Widgets und das Hören ohne Flutter-UI)
    // -----------------------------------------------------------------------

    /**
     * Eigener kleiner Thread-Pool für die Rust-Aufrufe, die ins NETZ gehen
     * können (Bibliothek durchsuchen, Warteschlange bauen, Scrobbeln).
     *
     * WARUM nicht [MediaCache.io]? Dieser eine Thread erledigt kurze
     * Datenbank-Zugriffe (Download-Index). Hinge er mehrere Sekunden an einem
     * langsamen Server, stünde auch das Auflösen der Titel und die
     * Download-Liste der Oberfläche still.
     *
     * AKKU: Bis zu 3 Threads, die sich nach 30 s Leerlauf selbst beenden
     * (allowCoreThreadTimeOut). Daemon-Threads halten den Prozess nicht am Leben.
     */
    val executor: ListeningExecutorService by lazy {
        val counter = AtomicInteger()
        val pool = ThreadPoolExecutor(
            3, 3, 30, TimeUnit.SECONDS, LinkedBlockingQueue(),
        ) { r -> Thread(r, "nocturne-net-${counter.incrementAndGet()}").apply { isDaemon = true } }
        pool.allowCoreThreadTimeOut(true)
        MoreExecutors.listeningDecorator(pool)
    }

    /**
     * Namen der `external`-Funktionen, die in der geladenen Bibliothek fehlen.
     *
     * WARUM? Die Rust-Seite dieser Funktionen kommt erst mit einem späteren
     * Build. Fehlt eine, wirft der ERSTE Aufruf `UnsatisfiedLinkError`. Wir
     * merken uns das und rufen sie danach gar nicht mehr auf – das spart das
     * Werfen und Loggen bei jedem Titelwechsel.
     */
    private val missing: MutableSet<String> = ConcurrentHashMap.newKeySet()

    /** Gemeinsame Hülle: Bibliothek da? Funktion bekannt? Fehler abfangen. */
    private inline fun <T> call(name: String, fallback: T, block: () -> T): T {
        if (!available || name in missing) return fallback
        return try {
            block()
        } catch (e: UnsatisfiedLinkError) {
            missing += name
            Log.w(TAG, "$name fehlt in der Rust-Bibliothek – wird ab jetzt übersprungen")
            fallback
        } catch (e: RuntimeException) {
            // Eine Rust-Panik o. ä., die als Java-Ausnahme ankommt: lieber
            // "nichts gefunden" als Absturz des Dienstes.
            Log.w(TAG, "$name fehlgeschlagen", e)
            fallback
        }
    }

    /** Kinder eines Ordners als JSON-Array (siehe [nativeBrowse]). Blockiert. */
    fun browse(parentId: String, german: Boolean): String =
        call("nativeBrowse", "[]") { nativeBrowse(parentId, german) ?: "[]" }

    /** Abspielbare Titel zu einem Suchbegriff als JSON-Array. Blockiert. */
    fun search(query: String): String =
        call("nativeSearch", "[]") { nativeSearch(query) ?: "[]" }

    /** Setzt in Rust die Warteschlange zu [mediaId]; `null` = unbekannt. Blockiert. */
    fun playFromMediaId(mediaId: String): String? =
        call("nativePlayFromMediaId", null) { nativePlayFromMediaId(mediaId)?.takeIf { it.isNotBlank() } }

    /** Gespeicherte Warteschlange samt Position; `null` = nichts gespeichert. Blockiert. */
    fun resume(): String? =
        call("nativeResume", null) { nativeResume()?.takeIf { it.isNotBlank() } }

    /** Schnell: darf auf dem Haupt-Thread laufen. */
    fun savePosition(positionMs: Long) =
        call("nativeSavePosition", Unit) { nativeSavePosition(positionMs) }

    /** Schnell. [kind]: [NET_NONE], [NET_UNMETERED] oder [NET_METERED]. */
    fun setNetwork(kind: Int) = call("nativeSetNetwork", Unit) { nativeSetNetwork(kind) }

    /** Titel wurde wirklich gehört. Blockiert evtl. (Netz) -> nur auf [executor]. */
    fun reportPlay(entryId: String, listenedMs: Long) =
        call("nativeReportPlay", Unit) { nativeReportPlay(entryId, listenedMs) }

    /** "Läuft gerade" an den Server. Blockiert evtl. (Netz) -> nur auf [executor]. */
    fun nowPlaying(entryId: String) = call("nativeNowPlaying", Unit) { nativeNowPlaying(entryId) }

    /**
     * Titelwechsel OHNE laufende Oberfläche: Rust führt seine Warteschlange
     * nach. Liefert die neue Liste als JSON, wenn der Player sie übernehmen
     * muss (z. B. neu gemischte Runde bei "Alle wiederholen"), sonst `null`.
     * Schreibt eine kleine Datei -> nicht auf dem Haupt-Thread aufrufen.
     */
    fun transition(entryId: String, wrapped: Boolean): String? =
        call("nativeTransition", null) {
            nativeTransition(entryId, wrapped)?.takeIf { it.isNotBlank() }
        }

    /** Werte für [setNetwork] (Vertrag mit Rust). */
    const val NET_NONE = 0
    const val NET_UNMETERED = 1
    const val NET_METERED = 2

    /** Ersetzt das frühere `NativeDsp.nativeInit`. [storageKey]: 32 Byte, siehe SecretStore. */
    @JvmStatic external fun nativeInit(dataDir: String, storageKey: ByteArray)

    @JvmStatic external fun nativeStreamUrl(songId: String): String?

    @JvmStatic external fun nativeCoverUrl(coverId: String, size: Int): String?

    // HINWEIS zu den Rückgabetypen: Rust liefert laut Vertrag `String` (nie
    // null). Wir deklarieren trotzdem `String?`: Für JNI ist das dieselbe
    // Signatur, aber ein versehentliches null aus Rust führt dann nicht zu
    // einer NullPointerException mitten im Dienst.

    /** JSON-Array der Kinder von [parentId]; Wurzel = "root". Bei Fehler "[]". */
    @JvmStatic external fun nativeBrowse(parentId: String, german: Boolean): String?

    /** Wie [nativeBrowse], nur abspielbare Titel. */
    @JvmStatic external fun nativeSearch(query: String): String?

    /** JSON {"index","positionMs","items":[…]} oder null. */
    @JvmStatic external fun nativePlayFromMediaId(mediaId: String): String?

    /** Wie [nativePlayFromMediaId], mit gespeicherter Position; null = nichts gespeichert. */
    @JvmStatic external fun nativeResume(): String?

    @JvmStatic external fun nativeSavePosition(positionMs: Long)

    @JvmStatic external fun nativeSetNetwork(kind: Int)

    @JvmStatic external fun nativeReportPlay(entryId: String, listenedMs: Long)

    @JvmStatic external fun nativeNowPlaying(entryId: String)

    /** JSON wie [nativePlayFromMediaId] oder null (= Liste im Player stimmt noch). */
    @JvmStatic external fun nativeTransition(entryId: String, wrapped: Boolean): String?
}
