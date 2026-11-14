package io.github.lama072004.nocturne

import android.app.Notification
import androidx.annotation.OptIn
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.offline.Download
import androidx.media3.exoplayer.offline.DownloadManager
import androidx.media3.exoplayer.offline.DownloadNotificationHelper
import androidx.media3.exoplayer.offline.DownloadService
import androidx.media3.exoplayer.scheduler.Scheduler
import androidx.media3.exoplayer.workmanager.WorkManagerScheduler

/**
 * Lädt Titel im Hintergrund herunter.
 *
 * WARUM ein eigener Dienst?
 *  - Downloads sollen weiterlaufen, wenn die App geschlossen wird. Android
 *    erlaubt das nur einem Foreground-Service mit sichtbarer Benachrichtigung.
 *  - Bricht ein Download ab (kein WLAN mehr, Handy-Neustart), plant der
 *    WorkManager-Scheduler die Fortsetzung, sobald die Bedingungen wieder
 *    passen. Bis dahin läuft NICHTS im Hintergrund – das schont den Akku.
 *
 * Der Dienst beendet sich selbst, sobald keine Downloads mehr anstehen.
 */
@OptIn(UnstableApi::class)
class MusicDownloadService : DownloadService(
    FOREGROUND_NOTIFICATION_ID,
    // Die Fortschritts-Benachrichtigung wird nur einmal pro Sekunde erneuert.
    DEFAULT_FOREGROUND_NOTIFICATION_UPDATE_INTERVAL,
    CHANNEL_ID,
    R.string.download_channel_name,
    0,
) {
    override fun getDownloadManager(): DownloadManager = MediaCache.downloadManager(this)

    override fun getScheduler(): Scheduler = WorkManagerScheduler(this, "music_downloads")

    override fun getForegroundNotification(
        downloads: MutableList<Download>,
        notMetRequirements: Int,
    ): Notification = DownloadNotificationHelper(this, CHANNEL_ID).buildProgressNotification(
        this,
        android.R.drawable.stat_sys_download,
        /* contentIntent = */ null,
        /* message = */ null,
        downloads,
        notMetRequirements,
    )

    companion object {
        private const val FOREGROUND_NOTIFICATION_ID = 2001
        private const val CHANNEL_ID = "downloads"
    }
}
