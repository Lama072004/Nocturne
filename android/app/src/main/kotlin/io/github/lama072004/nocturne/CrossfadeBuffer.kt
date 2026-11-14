package io.github.lama072004.nocturne

import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.cos
import kotlin.math.sin

/**
 * Der Rechenkern für das Überblenden (Crossfade) zwischen zwei Titeln.
 *
 * Bewusst OHNE Android- und Media3-Klassen geschrieben: So lässt sich die
 * Logik mit einem gewöhnlichen Unit-Test prüfen (siehe CrossfadeBufferTest),
 * ohne Gerät und ohne Hinhören. Die Anbindung an ExoPlayer steht in
 * [CrossfadeProcessor].
 *
 * # Idee
 *
 * Um das ENDE eines Titels mit dem ANFANG des nächsten mischen zu können,
 * muss man das Ende noch in der Hand haben, wenn der nächste Titel beginnt.
 * Deshalb läuft der Ton durch eine Verzögerung: Wir halten immer die letzten
 * X Sekunden zurück ([delayBytes]).
 *
 * ```text
 *  Eingang ──► [ Warteschlange: die letzten X Sekunden ] ──► Ausgang
 * ```
 *
 * Kommt die Titelgrenze ([markBoundary]), liegt in der Warteschlange genau
 * das Ende des alten Titels. Es wandert in den "Ausklang" ([tail]). Der neue
 * Titel füllt die Warteschlange neu; sobald er herauskommt, wird der Ausklang
 * daruntergemischt – der alte Titel wird dabei leiser, der neue lauter.
 *
 * Der Ausgang ist dadurch je Übergang um X Sekunden KÜRZER als der Eingang:
 * Ende und Anfang erklingen ja gleichzeitig.
 *
 * # Warum die Verzögerung ALLMÄHLICH wächst
 * Die erste Fassung hielt nach Start, Spulen und jeder Titelgrenze erst
 * einmal X Sekunden komplett zurück, bevor wieder Ton herauskam. Im Test
 * lief dabei der Puffer der Tonausgabe leer ("underrun") – hörbar als
 * kurzer Aussetzer genau am Beginn der Überblendung. Jetzt wird von jedem
 * ankommenden Block nur die HÄLFTE zurückgehalten, bis X Sekunden beisammen
 * sind ([currentDelay]). Die Tonausgabe bekommt also immer sofort Nachschub,
 * der Ton startet ohne Wartezeit, und der Player lädt in dieser Phase
 * einfach doppelt so schnell nach (was er aus seinem Vorrat problemlos kann).
 *
 * # Format
 * 16-Bit-PCM, verschachtelt (links, rechts, links, …), Little-Endian – das
 * Format, das ExoPlayer auf Android liefert.
 *
 * Nicht thread-sicher: Alle Aufrufe kommen vom Wiedergabe-Thread.
 */
class CrossfadeBuffer {

    /** Bytes je Frame (ein Sample für jeden Kanal). */
    private var frameBytes = 4
    private var sampleRate = 44_100
    private var channels = 2

    /** So viele Bytes sollen zurückgehalten werden (Ziel); 0 = Crossfade aus. */
    var delayBytes = 0
        private set

    /**
     * So viele Bytes werden GERADE zurückgehalten. Wächst mit jedem Block um
     * dessen halbe Länge bis [delayBytes] (siehe Klassenkommentar).
     */
    private var currentDelay = 0

    // --- Warteschlange als Ringpuffer --------------------------------------
    // Ringpuffer statt "vorne abschneiden": Kein Verschieben von Megabytes
    // bei jedem Tonblock.
    private var ring = ByteArray(0)
    private var head = 0
    private var size = 0

    // --- Ausklang des vorigen Titels ---------------------------------------
    private var tail = ByteArray(0)
    private var tailLen = 0
    private var tailPos = 0

    /** Stellt das Tonformat ein. Passt es nicht zum Gepufferten, wird geleert. */
    fun configure(sampleRate: Int, channels: Int) {
        if (sampleRate == this.sampleRate && channels == this.channels) return
        // Ein Ausklang mit anderer Abtastrate würde zu schnell oder zu
        // langsam (und damit in falscher Tonhöhe) daruntergemischt.
        clear()
        this.sampleRate = sampleRate
        this.channels = channels
        frameBytes = channels * 2
        delayBytes = 0
    }

    /** Dauer der Überblendung. 0 schaltet sie ab (Rest wird noch ausgegeben). */
    fun setDelayMs(ms: Int) {
        val frames = sampleRate.toLong() * ms.coerceIn(0, MAX_MS) / 1000
        delayBytes = (frames * frameBytes).toInt()
        // Kürzer gestellt oder aus: Das Zuviel kommt beim nächsten Block heraus.
        if (currentDelay > delayBytes) currentDelay = delayBytes
    }

    /** `true`: nichts gepuffert, nichts zu mischen, nichts zu verzögern. */
    val isPassThrough: Boolean
        get() = delayBytes == 0 && size == 0 && tailPos >= tailLen

    /** Hängt Ton hinten an die Warteschlange an (liest [src] vollständig). */
    fun push(src: ByteBuffer) {
        val n = src.remaining()
        if (n == 0) return
        ensureCapacity(size + n)
        var write = (head + size) % ring.size
        var left = n
        while (left > 0) {
            val chunk = minOf(left, ring.size - write)
            src.get(ring, write, chunk)
            write = (write + chunk) % ring.size
            left -= chunk
        }
        size += n
        // Verzögerung aufbauen: höchstens die Hälfte dieses Blocks behalten.
        if (currentDelay < delayBytes) {
            val half = n / 2
            currentDelay = minOf(delayBytes, currentDelay + half - half % frameBytes)
        }
    }

    /** So viele Bytes dürfen jetzt hinaus (alles über der Verzögerung). */
    fun available(): Int {
        val over = size - currentDelay
        // Nur ganze Frames ausgeben, sonst gerieten links/rechts durcheinander.
        return if (over <= 0) 0 else over - over % frameBytes
    }

    /** Alles, was noch in der Warteschlange liegt (für das Ende der Wiedergabe). */
    fun pending(): Int = size - size % frameBytes

    /**
     * Schreibt [count] Bytes nach [dst] (ab dessen Position) und mischt dabei
     * den Ausklang des vorigen Titels darunter, solange noch welcher da ist.
     */
    fun pop(dst: ByteBuffer, count: Int) {
        require(count in 0..size) { "Mehr angefordert als vorhanden" }
        val start = dst.position()
        var left = count
        while (left > 0) {
            val chunk = minOf(left, ring.size - head)
            dst.put(ring, head, chunk)
            head = (head + chunk) % ring.size
            left -= chunk
        }
        size -= count
        if (size == 0) head = 0
        if (tailPos < tailLen) mixTail(dst, start, count)
    }

    /**
     * Mischt den Ausklang in die eben geschriebenen Bytes.
     *
     * Lautstärkeverlauf "gleiche Leistung": Der alte Titel folgt einer
     * Kosinus-, der neue einer Sinuskurve. Bei einer geraden Linie (50 % +
     * 50 % in der Mitte) wirkte die Mitte der Überblendung hörbar leiser,
     * weil sich zwei verschiedene Musikstücke nicht einfach addieren.
     */
    private fun mixTail(dst: ByteBuffer, start: Int, count: Int) {
        // Eigene Sicht mit fester Byte-Reihenfolge; Position/Grenzen von dst
        // bleiben unberührt.
        val view = dst.duplicate().order(ByteOrder.LITTLE_ENDIAN)
        // Alle Längen sind Vielfache eines Frames (siehe available/pending/
        // markBoundary), deshalb kann frameweise gezählt werden.
        val n = minOf(count, tailLen - tailPos)
        val total = tailLen.toDouble()
        var i = 0
        while (i + frameBytes <= n) {
            // Fortschritt 0..1; alle Kanäle eines Frames bekommen denselben
            // Wert, sonst wanderte der Klang beim Überblenden zur Seite.
            val progress = (tailPos + i) / total
            val gainOld = cos(progress * HALF_PI)
            val gainNew = sin(progress * HALF_PI)
            var c = 0
            while (c < frameBytes) {
                val t = tailPos + i + c
                val old = ((tail[t + 1].toInt() shl 8) or (tail[t].toInt() and 0xFF)).toShort()
                val new = view.getShort(start + i + c)
                val mixed = old * gainOld + new * gainNew
                // Begrenzen statt überlaufen: Ein Überlauf klänge wie ein Knall.
                view.putShort(
                    start + i + c,
                    mixed.coerceIn(Short.MIN_VALUE.toDouble(), Short.MAX_VALUE.toDouble())
                        .toInt().toShort(),
                )
                c += 2
            }
            i += frameBytes
        }
        tailPos += n
        if (tailPos >= tailLen) {
            tailLen = 0
            tailPos = 0
        }
    }

    /**
     * Titelgrenze: Was jetzt in der Warteschlange liegt, ist das Ende des
     * alten Titels und wird zum Ausklang.
     *
     * Ein noch nicht aufgebrauchter älterer Ausklang (Titel kürzer als die
     * Überblendung) wird verworfen – drei Titel gleichzeitig wären Lärm.
     *
     * @return Anzahl der Frames, die zum Ausklang wurden (0 = nichts zu tun).
     */
    fun markBoundary(): Int {
        if (size < frameBytes) return 0
        val n = size - size % frameBytes
        if (tail.size < n) tail = ByteArray(n)
        var read = head
        var left = n
        var written = 0
        while (left > 0) {
            val chunk = minOf(left, ring.size - read)
            System.arraycopy(ring, read, tail, written, chunk)
            read = (read + chunk) % ring.size
            written += chunk
            left -= chunk
        }
        tailLen = n
        tailPos = 0
        head = 0
        size = 0
        // Der neue Titel beginnt ohne Verzögerung (sein Anfang soll sofort
        // unter den Ausklang) und baut sie dann wieder auf.
        currentDelay = 0
        return n / frameBytes
    }

    /** Alles verwerfen (nach dem Spulen oder Titel-Überspringen). */
    fun clear() {
        head = 0
        size = 0
        tailLen = 0
        tailPos = 0
        currentDelay = 0
    }

    /** Speicher freigeben (Player wird beendet oder Crossfade ist lange aus). */
    fun release() {
        clear()
        ring = ByteArray(0)
        tail = ByteArray(0)
    }

    private fun ensureCapacity(needed: Int) {
        if (needed <= ring.size) return
        // Großzügig wachsen, damit das selten passiert.
        val grown = ByteArray(maxOf(needed, ring.size * 2, 64 * 1024))
        var read = head
        var left = size
        var written = 0
        while (left > 0) {
            val chunk = minOf(left, ring.size - read)
            System.arraycopy(ring, read, grown, written, chunk)
            read = (read + chunk) % ring.size
            written += chunk
            left -= chunk
        }
        ring = grown
        head = 0
    }

    companion object {
        /** Längste erlaubte Überblendung (wie bei Spotify: 12 s). */
        const val MAX_MS = 12_000
        private const val HALF_PI = Math.PI / 2
    }
}
