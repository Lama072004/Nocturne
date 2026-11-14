package io.github.lama072004.nocturne

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

/**
 * Einstiegspunkt der App. Die Activity enthält nur die Flutter-UI.
 *
 * Die Wiedergabe läuft bewusst NICHT hier, sondern im PlaybackService:
 * Die Activity wird von Android beendet, sobald die App im Hintergrund ist –
 * die Musik soll aber weiterlaufen.
 *
 * Kanäle (neben PlayerBridge und DownloadBridge):
 *  - MethodChannel "musicplayer/system":
 *      storageKey -> ByteArray (32 Byte, siehe SecretStore). Dart reicht ihn
 *      an Rust weiter, damit Rust die Anmeldedaten verschlüsselt ablegen
 *      kann. Es ist derselbe Schlüssel, den der Dienst über
 *      NativeCore.nativeInit übergibt.
 */
class MainActivity : FlutterActivity() {

    private var playerBridge: PlayerBridge? = null
    private var downloadBridge: DownloadBridge? = null
    private var systemChannel: MethodChannel? = null
    private val main = Handler(Looper.getMainLooper())

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        askForNotificationPermission()
    }

    /**
     * Ab Android 13 braucht jede Benachrichtigung die Zustimmung des Nutzers.
     * Ohne sie wäre der Fortschritt der Downloads unsichtbar. (Die
     * Mediensteuerung selbst zeigt Android auch ohne diese Erlaubnis.)
     * Android merkt sich die Antwort; gefragt wird nur, solange noch keine
     * Entscheidung getroffen wurde.
     */
    private fun askForNotificationPermission() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU) return
        val permission = Manifest.permission.POST_NOTIFICATIONS
        if (checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(permission), 1)
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        val messenger = flutterEngine.dartExecutor.binaryMessenger
        playerBridge = PlayerBridge(applicationContext, messenger).also { it.attach() }
        downloadBridge = DownloadBridge(applicationContext, messenger).also { it.attach() }
        systemChannel = MethodChannel(messenger, "musicplayer/system").also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    // Der KeyStore-Zugriff dauert einige Millisekunden und
                    // kann beim ersten Mal einen Schlüssel erzeugen -> nicht
                    // auf dem UI-Thread.
                    "storageKey" -> MediaCache.io.execute {
                        try {
                            val key = SecretStore.key(applicationContext)
                            main.post { if (systemChannel === channel) result.success(key) }
                        } catch (e: Exception) {
                            main.post {
                                if (systemChannel === channel) {
                                    result.error("keystore_failed", e.message, null)
                                }
                            }
                        }
                    }
                    else -> result.notImplemented()
                }
            }
        }
    }

    override fun cleanUpFlutterEngine(flutterEngine: FlutterEngine) {
        playerBridge?.detach()
        playerBridge = null
        downloadBridge?.detach()
        downloadBridge = null
        systemChannel?.setMethodCallHandler(null)
        systemChannel = null
        super.cleanUpFlutterEngine(flutterEngine)
    }
}
