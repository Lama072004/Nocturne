package io.github.lama072004.nocturne

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.os.Bundle
import android.util.Log
import android.view.View
import android.widget.RemoteViews
import java.io.File
import java.util.Locale
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * Homescreen-Widgets (wie bei Spotify): drei Größen "Läuft gerade" und ein
 * Schnellzugriff auf zuletzt Gehörtes.
 *
 *  - [SmallPlayerWidget]  (1×1 / 2×1): Cover + Play/Pause
 *  - [MediumPlayerWidget] (4×1): Cover, Titel, Künstler, Zurück · Play/Pause · Weiter
 *  - [LargePlayerWidget]  (4×2): wie mittel, größeres Cover, Fortschrittsbalken,
 *                               Hintergrund in gedämpfter Cover-Farbe
 *  - [QuickAccessWidget]  (4×2): bis zu 6 Einträge aus nativeBrowse("recent")
 *
 * AKKU: Es gibt KEINE regelmäßige Aktualisierung (updatePeriodMillis = 0).
 * Neu gezeichnet wird nur, wenn der PlaybackService ein Ereignis meldet
 * (Titelwechsel, Play/Pause, Stopp) oder Android das Widget neu aufbaut
 * (Platzieren, Größe ändern, Neustart). Der Fortschrittsbalken zeigt deshalb
 * den Stand beim letzten Ereignis und läuft nicht mit.
 *
 * Läuft kein Dienst, zeigen die Widgets den zuletzt bekannten Titel aus
 * SharedPreferences bzw. "Nichts wird abgespielt".
 *
 * KNÖPFE: Alle gehen an den [WidgetActionReceiver] (nicht exportiert). Der
 * steuert den laufenden Dienst direkt oder baut – wenn keiner läuft – eine
 * MediaController-Verbindung auf (siehe dort, warum nicht MediaButtonReceiver).
 */
object HomeWidgets {
    private const val TAG = "HomeWidgets"
    private const val PREFS = "home_widgets"
    private const val K_HAS = "has"
    private const val K_TITLE = "title"
    private const val K_ARTIST = "artist"
    private const val K_COVER = "cover"
    private const val K_PLAYING = "playing"
    private const val K_POS = "pos"
    private const val K_DUR = "dur"
    private const val K_RECENT = "recent"

    const val ACTION_PLAY_PAUSE = "io.github.lama072004.nocturne.widget.PLAY_PAUSE"
    const val ACTION_NEXT = "io.github.lama072004.nocturne.widget.NEXT"
    const val ACTION_PREVIOUS = "io.github.lama072004.nocturne.widget.PREVIOUS"
    const val ACTION_PLAY_ID = "io.github.lama072004.nocturne.widget.PLAY_ID"
    const val EXTRA_MEDIA_ID = "mediaId"

    /** Kantenlänge des großen Covers in Pixeln. Klein genug fürs
     *  RemoteViews-Limit (Bitmaps reisen per Binder zum Launcher). */
    private const val COVER_PX = 320
    private const val QUICK_COVER_PX = 112
    private const val QUICK_CELLS = 6

    /** Dunkler Standard-Hintergrund, wenn kein Cover da ist. */
    private val DEFAULT_BG = Color.rgb(0x1C, 0x1B, 0x22)

    /** Was die Player-Widgets zeigen. */
    data class NowPlaying(
        val hasItem: Boolean,
        val title: String?,
        val artist: String?,
        val coverId: String?,
        val playing: Boolean,
        val positionMs: Long,
        val durationMs: Long,
    )

    /**
     * Ein Thread fürs Zeichnen (Cover laden/verkleinern). Einer genügt und
     * hält die Reihenfolge ein: Ein späteres Ereignis überholt nie ein
     * früheres. Daemon: hält den Prozess nicht am Leben.
     */
    private val executor: ExecutorService by lazy {
        Executors.newSingleThreadExecutor { r ->
            Thread(r, "nocturne-widgets").apply { isDaemon = true }
        }
    }

    /** Zuletzt geladenes Cover (nur auf [executor] benutzt). Spart bei
     *  Play/Pause das erneute Lesen und Verkleinern. */
    private var coverCache: Pair<String, Bitmap>? = null

    private fun prefs(context: Context) =
        context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

    // -----------------------------------------------------------------------
    // Einstieg vom Dienst und von den Providern
    // -----------------------------------------------------------------------

    /** Vom PlaybackService (Haupt-Thread): Zustand merken und zeichnen. */
    fun publish(context: Context, state: NowPlaying) {
        val app = context.applicationContext
        // Merken für die Zeit ohne Dienst (apply = asynchron, blockiert nicht).
        prefs(app).edit()
            .putBoolean(K_HAS, state.hasItem)
            .putString(K_TITLE, state.title)
            .putString(K_ARTIST, state.artist)
            .putString(K_COVER, state.coverId)
            .putBoolean(K_PLAYING, state.playing)
            .putLong(K_POS, state.positionMs)
            .putLong(K_DUR, state.durationMs)
            .apply()
        executor.execute { safely { renderPlayers(app, state) } }
    }

    /** Für die Provider: zuletzt bekannten Zustand zeichnen, dann [done]. */
    fun renderSaved(context: Context, done: () -> Unit) {
        val app = context.applicationContext
        executor.execute {
            try {
                safely { renderPlayers(app, loadSaved(app)) }
            } finally {
                done()
            }
        }
    }

    private fun loadSaved(app: Context): NowPlaying {
        val p = prefs(app)
        return NowPlaying(
            hasItem = p.getBoolean(K_HAS, false),
            title = p.getString(K_TITLE, null),
            artist = p.getString(K_ARTIST, null),
            coverId = p.getString(K_COVER, null),
            // Läuft kein Dienst, kann auch nichts spielen – egal, was zuletzt
            // gespeichert wurde (der Prozess kann ohne onDestroy beendet sein).
            playing = p.getBoolean(K_PLAYING, false) && PlaybackService.running() != null,
            positionMs = p.getLong(K_POS, 0L),
            durationMs = p.getLong(K_DUR, 0L),
        )
    }

    /** Schnellzugriff aus dem Zwischenspeicher zeichnen (ohne Netz). */
    fun renderQuickSaved(context: Context) {
        val app = context.applicationContext
        safely { renderQuick(app, MediaItems.parseBrowseEntries(prefs(app).getString(K_RECENT, null))) }
    }

    /**
     * Holt "zuletzt gehört" frisch aus Rust und zeichnet den Schnellzugriff.
     * BLOCKIERT (Netz) -> nur im Hintergrund. Tut nichts, wenn kein
     * Schnellzugriff-Widget existiert (kein unnötiger Netzzugriff).
     */
    fun refreshQuickAccess(context: Context) {
        val app = context.applicationContext
        safely {
            if (ids(app, QuickAccessWidget::class.java).isEmpty()) return@safely
            if (!NativeCore.ensureInit(app)) return@safely
            val german = Locale.getDefault().language == "de"
            val json = NativeCore.browse("recent", german)
            val entries = MediaItems.parseBrowseEntries(json)
            // Leer = evtl. nur Fehler (Rust liefert dann "[]") -> den alten
            // Stand behalten statt das Widget zu leeren.
            if (entries.isEmpty()) return@safely
            prefs(app).edit().putString(K_RECENT, json).apply()
            renderQuick(app, entries)
        }
    }

    /** Ein Widget darf nie den Prozess (= die Musik) mitreißen. */
    private inline fun safely(block: () -> Unit) {
        try {
            block()
        } catch (e: RuntimeException) {
            Log.w(TAG, "Widget konnte nicht gezeichnet werden", e)
        } catch (e: OutOfMemoryError) {
            Log.w(TAG, "Zu wenig Speicher fürs Widget-Cover", e)
        }
    }

    private fun ids(app: Context, cls: Class<*>): IntArray =
        AppWidgetManager.getInstance(app).getAppWidgetIds(ComponentName(app, cls))

    // -----------------------------------------------------------------------
    // Zeichnen: Player-Widgets
    // -----------------------------------------------------------------------

    private fun renderPlayers(app: Context, s: NowPlaying) {
        val small = ids(app, SmallPlayerWidget::class.java)
        val medium = ids(app, MediumPlayerWidget::class.java)
        val large = ids(app, LargePlayerWidget::class.java)
        // Kein Widget platziert -> keine Arbeit (das ist der Normalfall).
        if (small.isEmpty() && medium.isEmpty() && large.isEmpty()) return

        val cover = if (s.hasItem) s.coverId?.let { mainCover(app, it) } else null
        val mgr = AppWidgetManager.getInstance(app)

        if (small.isNotEmpty()) {
            val v = RemoteViews(app.packageName, R.layout.widget_small)
            bindCover(app, v, cover)
            bindPlayButton(app, v, s)
            mgr.updateAppWidget(small, v)
        }
        if (medium.isNotEmpty()) {
            val v = RemoteViews(app.packageName, R.layout.widget_medium)
            bindCover(app, v, cover)
            bindPlayButton(app, v, s)
            bindTextAndSkip(app, v, s)
            mgr.updateAppWidget(medium, v)
        }
        if (large.isNotEmpty()) {
            val v = RemoteViews(app.packageName, R.layout.widget_large)
            bindCover(app, v, cover)
            bindPlayButton(app, v, s)
            bindTextAndSkip(app, v, s)
            val progress = if (s.durationMs > 0) {
                (s.positionMs * 1000 / s.durationMs).toInt().coerceIn(0, 1000)
            } else 0
            v.setProgressBar(R.id.widget_progress, 1000, progress, false)
            // ImageView.setColorFilter(int) ist über RemoteViews erlaubt und
            // färbt die abgerundete Fläche – so bleiben die runden Ecken
            // erhalten (setBackgroundColor würde sie überdecken).
            v.setInt(R.id.widget_bg_tint, "setColorFilter", cover?.let(::mutedColor) ?: DEFAULT_BG)
            mgr.updateAppWidget(large, v)
        }
    }

    private fun bindCover(app: Context, v: RemoteViews, cover: Bitmap?) {
        if (cover != null) v.setImageViewBitmap(R.id.widget_cover, cover)
        else v.setImageViewResource(R.id.widget_cover, R.drawable.widget_cover_placeholder)
        // Tippen aufs Cover öffnet die App.
        v.setOnClickPendingIntent(R.id.widget_cover, openApp(app))
    }

    private fun bindPlayButton(app: Context, v: RemoteViews, s: NowPlaying) {
        v.setImageViewResource(
            R.id.widget_play,
            if (s.playing) R.drawable.ic_widget_pause else R.drawable.ic_widget_play,
        )
        v.setContentDescription(
            R.id.widget_play,
            app.getString(if (s.playing) R.string.cd_pause else R.string.cd_play),
        )
        v.setOnClickPendingIntent(R.id.widget_play, action(app, ACTION_PLAY_PAUSE, 1, null))
    }

    private fun bindTextAndSkip(app: Context, v: RemoteViews, s: NowPlaying) {
        val title = if (s.hasItem) s.title.orEmpty() else app.getString(R.string.widget_nothing_playing)
        val artist = if (s.hasItem) s.artist.orEmpty() else ""
        v.setTextViewText(R.id.widget_title, title)
        v.setTextViewText(R.id.widget_artist, artist)
        v.setViewVisibility(R.id.widget_artist, if (artist.isEmpty()) View.GONE else View.VISIBLE)
        v.setOnClickPendingIntent(R.id.widget_text, openApp(app))
        v.setOnClickPendingIntent(R.id.widget_prev, action(app, ACTION_PREVIOUS, 2, null))
        v.setOnClickPendingIntent(R.id.widget_next, action(app, ACTION_NEXT, 3, null))
    }

    // -----------------------------------------------------------------------
    // Zeichnen: Schnellzugriff
    // -----------------------------------------------------------------------

    private val CELL_IDS = intArrayOf(
        R.id.quick_cell_0, R.id.quick_cell_1, R.id.quick_cell_2,
        R.id.quick_cell_3, R.id.quick_cell_4, R.id.quick_cell_5,
    )
    private val COVER_IDS = intArrayOf(
        R.id.quick_cover_0, R.id.quick_cover_1, R.id.quick_cover_2,
        R.id.quick_cover_3, R.id.quick_cover_4, R.id.quick_cover_5,
    )
    private val TITLE_IDS = intArrayOf(
        R.id.quick_title_0, R.id.quick_title_1, R.id.quick_title_2,
        R.id.quick_title_3, R.id.quick_title_4, R.id.quick_title_5,
    )

    private fun renderQuick(app: Context, entries: List<MediaItems.BrowseEntry>) {
        val ids = ids(app, QuickAccessWidget::class.java)
        if (ids.isEmpty()) return
        val shown = entries.take(QUICK_CELLS)
        val v = RemoteViews(app.packageName, R.layout.widget_quick)
        v.setOnClickPendingIntent(R.id.widget_quick_header, openApp(app))
        v.setViewVisibility(R.id.widget_quick_empty, if (shown.isEmpty()) View.VISIBLE else View.GONE)
        v.setViewVisibility(R.id.widget_quick_grid, if (shown.isEmpty()) View.GONE else View.VISIBLE)
        for (i in 0 until QUICK_CELLS) {
            val e = shown.getOrNull(i)
            if (e == null) {
                // INVISIBLE statt GONE: Das Raster behält seine Aufteilung.
                v.setViewVisibility(CELL_IDS[i], View.INVISIBLE)
                continue
            }
            v.setViewVisibility(CELL_IDS[i], View.VISIBLE)
            v.setTextViewText(TITLE_IDS[i], e.title)
            val bmp = e.coverId?.let { coverBitmap(app, it, QUICK_COVER_PX) }
            if (bmp != null) v.setImageViewBitmap(COVER_IDS[i], bmp)
            else v.setImageViewResource(COVER_IDS[i], R.drawable.widget_cover_placeholder)
            // Tippen startet das Album/die Playlist: Der Dienst baut die
            // Warteschlange über nativePlayFromMediaId.
            v.setOnClickPendingIntent(CELL_IDS[i], action(app, ACTION_PLAY_ID, 100 + i, e.id))
        }
        AppWidgetManager.getInstance(app).updateAppWidget(ids, v)
    }

    // -----------------------------------------------------------------------
    // PendingIntents
    // -----------------------------------------------------------------------

    /** FLAG_IMMUTABLE: Pflicht ab Android 12, und niemand soll den Intent
     *  nachträglich verändern können. */
    private const val PI_FLAGS = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT

    private fun openApp(app: Context): PendingIntent = PendingIntent.getActivity(
        app,
        0,
        Intent(app, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        PI_FLAGS,
    )

    /**
     * Knopf -> WidgetActionReceiver. Jeder Knopf/jede Zelle hat einen eigenen
     * requestCode: Sonst teilten sich alle PendingIntents dieselben Extras
     * (Android unterscheidet sie nicht an den Extras).
     */
    private fun action(app: Context, action: String, requestCode: Int, mediaId: String?): PendingIntent {
        val intent = Intent(app, WidgetActionReceiver::class.java).setAction(action)
        if (mediaId != null) intent.putExtra(EXTRA_MEDIA_ID, mediaId)
        return PendingIntent.getBroadcast(app, requestCode, intent, PI_FLAGS)
    }

    // -----------------------------------------------------------------------
    // Cover
    // -----------------------------------------------------------------------

    /** Großes Cover mit Zwischenspeicher (nur auf [executor] aufrufen). */
    private fun mainCover(app: Context, coverId: String): Bitmap? {
        coverCache?.let { (id, bmp) -> if (id == coverId) return bmp }
        val bmp = coverBitmap(app, coverId, COVER_PX) ?: return null
        coverCache = coverId to bmp
        return bmp
    }

    /**
     * Cover aus dem ArtworkProvider-Cache (lädt es notfalls einmal vom
     * Server), quadratisch zugeschnitten und auf [size] verkleinert.
     * BLOCKIERT -> nur im Hintergrund.
     */
    private fun coverBitmap(app: Context, coverId: String, size: Int): Bitmap? {
        val file: File = ArtworkProvider.coverFileBlocking(app, coverId) ?: return null
        // Erst nur die Maße lesen, dann mit passendem inSampleSize laden:
        // Ein 3000-px-Cover voll zu dekodieren kostete ~36 MB.
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.path, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        var sample = 1
        while (bounds.outWidth / (sample * 2) >= size && bounds.outHeight / (sample * 2) >= size) {
            sample *= 2
        }
        val raw = BitmapFactory.decodeFile(
            file.path,
            BitmapFactory.Options().apply {
                inSampleSize = sample
                inPreferredConfig = Bitmap.Config.ARGB_8888
            },
        ) ?: return null
        val side = minOf(raw.width, raw.height)
        val square = Bitmap.createBitmap(raw, (raw.width - side) / 2, (raw.height - side) / 2, side, side)
        return if (side > size) Bitmap.createScaledBitmap(square, size, size, true) else square
    }

    /**
     * Gedämpfte Durchschnittsfarbe des Covers für den Hintergrund des großen
     * Widgets. Auf 16×16 verkleinern und mitteln ist schnell und genügt;
     * danach Sättigung und Helligkeit begrenzen, damit weiße Schrift
     * lesbar bleibt.
     */
    private fun mutedColor(cover: Bitmap): Int {
        val tiny = Bitmap.createScaledBitmap(cover, 16, 16, true)
        val px = IntArray(16 * 16)
        tiny.getPixels(px, 0, 16, 0, 0, 16, 16)
        var r = 0L
        var g = 0L
        var b = 0L
        for (c in px) {
            r += Color.red(c)
            g += Color.green(c)
            b += Color.blue(c)
        }
        val n = px.size
        val hsv = FloatArray(3)
        Color.RGBToHSV((r / n).toInt(), (g / n).toInt(), (b / n).toInt(), hsv)
        hsv[1] = (hsv[1] * 0.7f).coerceAtMost(0.6f)
        hsv[2] = (hsv[2] * 0.45f).coerceIn(0.12f, 0.30f)
        return Color.HSVToColor(hsv)
    }
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

/**
 * Gemeinsame Basis der drei "Läuft gerade"-Widgets. Android ruft onUpdate
 * nur beim Platzieren, nach einem Neustart oder App-Update (kein
 * Zeitintervall). Dann zeichnen wir den zuletzt bekannten Zustand.
 */
abstract class PlayerWidgetBase : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, appWidgetIds: IntArray) {
        // goAsync: Das Laden des Covers darf länger dauern als onReceive.
        val pending = goAsync()
        HomeWidgets.renderSaved(context) { pending.finish() }
    }

    override fun onAppWidgetOptionsChanged(
        context: Context,
        manager: AppWidgetManager,
        appWidgetId: Int,
        newOptions: Bundle,
    ) {
        val pending = goAsync()
        HomeWidgets.renderSaved(context) { pending.finish() }
    }
}

/** Klein (1×1 / 2×1): Cover + Play/Pause. */
class SmallPlayerWidget : PlayerWidgetBase()

/** Mittel (4×1): Cover, Titel, Künstler, Zurück · Play/Pause · Weiter. */
class MediumPlayerWidget : PlayerWidgetBase()

/** Groß (4×2): wie mittel, dazu Fortschrittsbalken und Cover-Farbe. */
class LargePlayerWidget : PlayerWidgetBase()

/**
 * Schnellzugriff (4×2): zuletzt gehörte Alben/Playlists. Beim Platzieren
 * sofort den Zwischenstand zeigen, dann frisch aus Rust holen. Danach
 * aktualisiert der Dienst es, sobald ein Titel als gehört gemeldet wurde.
 */
class QuickAccessWidget : AppWidgetProvider() {
    override fun onUpdate(context: Context, manager: AppWidgetManager, appWidgetIds: IntArray) {
        val app = context.applicationContext
        val pending = goAsync()
        NativeCore.executor.execute {
            try {
                HomeWidgets.renderQuickSaved(app)
            } finally {
                // Früh freigeben: Der Netzabruf unten darf länger dauern als
                // ein Broadcast leben darf. Er ist nur "best effort".
                pending.finish()
            }
            HomeWidgets.refreshQuickAccess(app)
        }
    }
}
