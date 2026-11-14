package io.github.lama072004.nocturne

import android.content.Context
import android.content.SharedPreferences
import android.os.Environment
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.database.StandaloneDatabaseProvider
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.datasource.DefaultHttpDataSource
import androidx.media3.datasource.cache.CacheDataSource
import androidx.media3.datasource.cache.CacheKeyFactory
import androidx.media3.datasource.cache.LeastRecentlyUsedCacheEvictor
import androidx.media3.datasource.cache.NoOpCacheEvictor
import androidx.media3.datasource.cache.SimpleCache
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.scheduler.Requirements
import java.io.File
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * Zentrale Stelle für alles, was Audiodateien auf dem Gerät speichert.
 *
 * Es gibt ZWEI getrennte Speicher:
 *
 *  1. Download-Cache: Titel, die der Nutzer bewusst heruntergeladen hat.
 *     Wird nie automatisch geleert (NoOpCacheEvictor).
 *  2. Streaming-Cache: zuletzt gestreamte Titel. Hat eine Obergrenze; ist
 *     sie erreicht, fliegen die am längsten nicht gehörten Titel raus
 *     (LeastRecentlyUsed). Spart Datenvolumen beim erneuten Hören.
 *
 * WARUM Media3 dafür und kein eigener Download-Code in Rust?
 * Der Player liest über dieselbe Schicht: Liegt ein Titel im Cache, wird er
 * automatisch von dort gespielt – ohne dass irgendwo "ist heruntergeladen?"
 * abgefragt werden muss. Offline-Wiedergabe ergibt sich damit von selbst.
 * Außerdem bringt Media3 Hintergrund-Downloads mit Fortsetzen nach Abbruch mit.
 *
 * Alles hier sind Singletons: SimpleCache erlaubt pro Ordner nur EINE
 * Instanz im Prozess, sonst gibt es einen Absturz.
 */
@OptIn(UnstableApi::class)
object MediaCache {
    private const val PREFS = "media_cache"
    const val PREF_SD_CARD = "sd_card"
    const val PREF_WIFI_ONLY = "wifi_only"
    const val PREF_STREAM_CACHE_MB = "stream_cache_mb"

    /**
     * Der Ordner, den der Download-Cache in diesem Prozess TATSÄCHLICH nutzt.
     * Weicht von [PREF_SD_CARD] ab, wenn die Karte fehlt (Rückfall auf intern)
     * oder die Einstellung erst nach einem Neustart wirkt. Die Oberfläche
     * zeigt damit an, wo die Downloads wirklich liegen (`storageInfo`).
     */
    const val PREF_ACTIVE_DIR = "active_dir"

    /**
     * Gemeinsamer Hintergrund-Thread für kurze, blockierende Zugriffe
     * (Download-Index = SQLite, Schlüsselspeicher). Einer für die ganze App
     * statt einer pro Activity: Ein eigener Executor je Bridge würde bei
     * jedem Neuaufbau der Activity einen Thread übrig lassen.
     * Daemon-Thread: hält den Prozess nicht am Leben.
     */
    val io: ExecutorService by lazy {
        Executors.newSingleThreadExecutor { r ->
            Thread(r, "nocturne-io").apply { isDaemon = true }
        }
    }

    private var db: StandaloneDatabaseProvider? = null
    private var downloadCache: SimpleCache? = null
    private var streamCache: SimpleCache? = null
    private var downloadManager: DownloadManager? = null

    /**
     * Die Einstellungen liegen zusätzlich in SharedPreferences, weil dieser
     * Code auch OHNE laufende Flutter-UI startet (z. B. wenn Android den
     * Download-Dienst nach einem Neustart fortsetzt). Dann kann er Rust/Dart
     * nicht fragen.
     */
    fun prefs(context: Context): SharedPreferences =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    @Synchronized
    private fun database(context: Context): StandaloneDatabaseProvider =
        db ?: StandaloneDatabaseProvider(context.applicationContext).also { db = it }

    /**
     * Ordner für Downloads: interner App-Speicher oder SD-Karte.
     *
     * In beiden Fällen ist es ein APP-EIGENER Ordner. Dafür braucht es keine
     * Speicher-Berechtigung, und die Dateien werden bei der Deinstallation
     * entfernt. Ein Wechsel der Einstellung wirkt nach einem App-Neustart.
     */
    private fun downloadDir(context: Context): File {
        val app = context.applicationContext
        if (prefs(app).getBoolean(PREF_SD_CARD, false)) {
            // Index 0 ist der eingebaute "externe" Speicher, ab Index 1 kommen
            // echte Wechselmedien. Fehlt die Karte, fällt es auf intern zurück.
            sdCardRoot(app)?.let { return File(it, "downloads") }
        }
        return File(app.filesDir, "downloads")
    }

    /** App-eigener Ordner auf einer eingelegten, beschreibbaren SD-Karte. */
    private fun sdCardRoot(context: Context): File? =
        context.getExternalFilesDirs(null).drop(1).firstOrNull {
            it != null && Environment.getExternalStorageState(it) == Environment.MEDIA_MOUNTED
        }

    @Synchronized
    fun downloadCache(context: Context): SimpleCache {
        downloadCache?.let { return it }
        val dir = downloadDir(context)
        // Merken, wo der Cache WIRKLICH liegt (siehe PREF_ACTIVE_DIR).
        prefs(context).edit().putString(PREF_ACTIVE_DIR, dir.absolutePath).apply()
        return SimpleCache(dir, NoOpCacheEvictor(), database(context))
            .also { downloadCache = it }
    }

    /**
     * Für die Oberfläche: Wo liegen die Downloads gerade, und gäbe es eine
     * SD-Karte? Blockiert kurz (Dateisystem), daher auf [io] aufrufen.
     */
    fun storageInfo(context: Context): Map<String, Any> {
        val app = context.applicationContext
        // Sicherstellen, dass der Cache (und damit PREF_ACTIVE_DIR) steht.
        downloadCache(app)
        val active = prefs(app).getString(PREF_ACTIVE_DIR, null)
            ?: File(app.filesDir, "downloads").absolutePath
        val internal = app.filesDir.absolutePath
        return mapOf(
            "activeDir" to active,
            "sdCardAvailable" to (sdCardRoot(app) != null),
            // Alles außerhalb des internen App-Speichers ist die SD-Karte
            // (der Rückfall landet immer unter filesDir).
            "usingSdCard" to !active.startsWith(internal),
        )
    }

    /** `null`, wenn der Streaming-Cache in den Einstellungen ausgeschaltet ist. */
    @Synchronized
    private fun streamCache(context: Context): SimpleCache? {
        streamCache?.let { return it }
        val mb = prefs(context).getInt(PREF_STREAM_CACHE_MB, 512)
        if (mb <= 0) return null
        // cacheDir: Android darf diesen Ordner bei Speichermangel selbst leeren.
        // Für einen reinen Komfort-Cache ist das genau richtig.
        return SimpleCache(
            File(context.applicationContext.cacheDir, "stream"),
            LeastRecentlyUsedCacheEvictor(mb.toLong() * 1024 * 1024),
            database(context),
        ).also { streamCache = it }
    }

    /**
     * HTTP mit Prüfung auf Fehlermeldungen statt Audio (siehe
     * [AudioOnlyDataSource]). Gilt für Wiedergabe UND Downloads.
     */
    private fun http(): DataSource.Factory = AudioOnlyDataSource.Factory(
        DefaultHttpDataSource.Factory()
            .setUserAgent("Nocturne")
            .setAllowCrossProtocolRedirects(true),
    )

    /**
     * Schlüssel für den Streaming-Cache: Song-ID + Format + Bitrate.
     *
     * WARUM nicht nur die Song-ID? Derselbe Titel wird im WLAN evtl. als FLAC
     * und unterwegs als Opus gestreamt. Unter demselben Schlüssel würden sich
     * Teile beider Dateien mischen -> kaputter Ton.
     * (Die URL selbst taugt nicht als Schlüssel: Sie enthält Token und Salt,
     * die sich bei jeder Anmeldung ändern.)
     */
    private val streamKeyFactory = CacheKeyFactory { spec ->
        val id = spec.key ?: spec.uri.getQueryParameter("id") ?: spec.uri.toString()
        "$id|${spec.uri.getQueryParameter("format")}|${spec.uri.getQueryParameter("maxBitRate")}"
    }

    /**
     * Datenquelle für den Player. Die Reihenfolge der Suche:
     *
     *   Download-Cache  ->  Streaming-Cache  ->  Netzwerk
     *
     * Der Download-Cache nutzt als Schlüssel nur die Song-ID (siehe
     * `customCacheKey` beim MediaItem): Ein heruntergeladener Titel wird
     * immer genommen, egal in welcher Qualität gerade gestreamt würde.
     *
     * WICHTIG: Der Dienst setzt diesen Schlüssel NUR, wenn der Download
     * vollständig ist (PlaybackService.resolve). Ein halb geladener Download
     * (läuft noch, gestoppt, fehlgeschlagen) würde sonst Stück für Stück mit
     * Stream-Bytes in ANDERER Qualität aufgefüllt -> kaputter Ton. Ohne
     * Schlüssel fällt die Suche im Download-Cache ins Leere (Schlüssel wäre
     * die URL), und es greift nur der Streaming-Cache.
     */
    fun playbackDataSourceFactory(context: Context): DataSource.Factory {
        val network = DefaultDataSource.Factory(context, http())
        val stream = streamCache(context)
        val upstream: DataSource.Factory = if (stream == null) network else {
            CacheDataSource.Factory()
                .setCache(stream)
                .setUpstreamDataSourceFactory(network)
                .setCacheKeyFactory(streamKeyFactory)
                // Ist der Cache kaputt, lieber normal streamen als abbrechen.
                .setFlags(CacheDataSource.FLAG_IGNORE_CACHE_ON_ERROR)
        }
        return CacheDataSource.Factory()
            .setCache(downloadCache(context))
            .setUpstreamDataSourceFactory(upstream)
            // null = nur lesen. In den Download-Cache schreibt ausschließlich
            // der Download-Dienst, nie das normale Abspielen.
            .setCacheWriteDataSinkFactory(null)
    }

    fun requirements(context: Context): Requirements = Requirements(
        if (prefs(context).getBoolean(PREF_WIFI_ONLY, true)) Requirements.NETWORK_UNMETERED
        else Requirements.NETWORK,
    )

    @Synchronized
    fun downloadManager(context: Context): DownloadManager =
        downloadManager ?: DownloadManager(
            context.applicationContext,
            database(context),
            downloadCache(context),
            http(),
            // Eigener kleiner Thread-Pool, damit Downloads nie die Wiedergabe bremsen.
            Executors.newFixedThreadPool(2),
        ).apply {
            // Zwei gleichzeitig: schneller als einer, aber ohne den Server
            // (der evtl. jeden Titel umwandeln muss) zu überlasten.
            maxParallelDownloads = 2
            requirements = requirements(context)
        }.also { downloadManager = it }
}
