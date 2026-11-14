package io.github.lama072004.nocturne

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor.AudioFormat
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import androidx.media3.exoplayer.audio.AudioSink
import androidx.media3.exoplayer.audio.ForwardingAudioSink
import java.nio.ByteBuffer

/**
 * Überblenden (Crossfade) zwischen zwei Titeln, direkt im Audio-Pfad von
 * ExoPlayer. Die Rechnung selbst steht in [CrossfadeBuffer]; diese Klasse
 * übersetzt nur ExoPlayers Aufrufe.
 *
 * # Wie ExoPlayer einen lückenlosen Titelwechsel abwickelt
 *
 * Am Ende JEDES Titels – auch mitten in einer Playlist – macht ExoPlayers
 * Tonausgabe Folgendes mit allen Prozessoren:
 *
 *  1. `queueEndOfStream()`: "Gib alles heraus, was du noch hast."
 *  2. `flush()`: "Vergiss deinen Zustand."
 *  3. Danach kommen die ersten Daten des nächsten Titels.
 *
 * Für eine Überblendung ist genau das Gegenteil nötig: Das Ende des alten
 * Titels muss ZURÜCKGEHALTEN werden, bis der neue da ist. Deshalb:
 *
 *  - Bei Schritt 1 geben wir NICHTS heraus, sondern erklären das
 *    Zurückgehaltene zum "Ausklang" (= Titelgrenze).
 *  - Bei Schritt 2 behalten wir den Ausklang.
 *
 * Zwei Fälle müssen davon ausgenommen sein, und beide erkennt nur die
 * Tonausgabe selbst – dafür gibt es [wrap]:
 *
 *  - ECHTES Ende (letzter Titel der Warteschlange, Sleep-Timer "bis
 *    Titelende"): Dann muss der Rest sehr wohl hinaus, sonst fehlten die
 *    letzten Sekunden. ExoPlayer ruft dafür `playToEndOfStream()`.
 *  - Spulen oder Titel überspringen: Dann ist das Zurückgehaltene wertlos
 *    und muss weg. ExoPlayer ruft dafür `flush()` auf der Tonausgabe.
 *
 * # Stimmt die angezeigte Position noch?
 * Ja, ohne Zutun: ExoPlayer rechnet die Position aus den tatsächlich
 * ausgegebenen Samples und verankert sie bei jedem Titelwechsel neu am
 * Beginn des neuen Titels. Der Wechsel in der Anzeige fällt damit genau auf
 * den Beginn der Überblendung (wie bei Spotify).
 *
 * # Nebenwirkungen (bewusst in Kauf genommen)
 *  - Hardware-Offload ist nicht möglich, solange Crossfade an ist (der Ton
 *    muss durch diese Klasse laufen) -> etwas mehr Akku. Standard ist "aus".
 *  - Kurz nach Start, Spulen und Titelwechsel ist die volle Dauer noch nicht
 *    aufgebaut (siehe CrossfadeBuffer). Wechselt der Titel in dieser Zeit,
 *    fällt die Überblendung entsprechend kürzer aus.
 *  - Sleep-Timer "bis Titelende" und das letzte Lied der Warteschlange
 *    klingen ganz aus, ohne Überblendung.
 */
@OptIn(UnstableApi::class)
class CrossfadeProcessor : BaseAudioProcessor() {

    /**
     * Dauer der Überblendung in Millisekunden; 0 = aus. Wird vom
     * Haupt-Thread gesetzt und vom Wiedergabe-Thread gelesen.
     */
    @Volatile
    var crossfadeMs = 0

    private val buffer = CrossfadeBuffer()

    /** `true`: Das nächste "Ende" ist das echte Ende der Wiedergabe. */
    private var finalDrain = false

    override fun onConfigure(inputAudioFormat: AudioFormat): AudioFormat {
        // Wie bei RustDspProcessor: nur 16-Bit-PCM (ExoPlayer wandelt vorher
        // um). Alles andere läuft unverändert an uns vorbei.
        if (inputAudioFormat.encoding != C.ENCODING_PCM_16BIT) return AudioFormat.NOT_SET
        return inputAudioFormat
    }

    override fun queueInput(inputBuffer: ByteBuffer) {
        val size = inputBuffer.remaining()
        if (size == 0) return
        // Neue Daten -> ein früher angekündigtes "echtes Ende" gilt nicht mehr.
        finalDrain = false
        buffer.setDelayMs(crossfadeMs)

        if (buffer.isPassThrough) {
            // Crossfade aus und nichts mehr gepuffert: nur durchreichen.
            replaceOutputBuffer(size).put(inputBuffer).flip()
            return
        }
        buffer.push(inputBuffer)
        emit(buffer.available())
    }

    override fun onQueueEndOfStream() {
        if (finalDrain) {
            // Echtes Ende: Der Titel soll bis zum letzten Ton ausklingen.
            emit(buffer.pending())
        } else {
            // Titelwechsel mitten in der Warteschlange (siehe Klassen-
            // kommentar): zurückhalten und als Ausklang vormerken.
            buffer.markBoundary()
        }
    }

    override fun onFlush() {
        // KEIN buffer.clear() hier: flush() kommt auch bei jedem lückenlosen
        // Titelwechsel. Nur das Format nachziehen – ändert es sich, verwirft
        // der Puffer von selbst (ein Ausklang in fremder Abtastrate wäre
        // falsch). Echtes Leeren macht [hardReset].
        buffer.configure(inputAudioFormat.sampleRate, inputAudioFormat.channelCount)
    }

    override fun onReset() {
        buffer.release()
        finalDrain = false
    }

    /** Spulen/Überspringen: alles Zurückgehaltene verwerfen. */
    private fun hardReset() {
        buffer.clear()
        finalDrain = false
    }

    private fun emit(count: Int) {
        if (count <= 0) return
        val out = replaceOutputBuffer(count)
        buffer.pop(out, count)
        out.flip()
    }

    /**
     * Legt eine dünne Hülle um die Tonausgabe, die diesem Prozessor die zwei
     * Dinge sagt, die er aus seiner eigenen Sicht nicht unterscheiden kann
     * (siehe Klassenkommentar).
     */
    fun wrap(sink: AudioSink): AudioSink = object : ForwardingAudioSink(sink) {
        override fun flush() {
            hardReset()
            super.flush()
        }

        override fun playToEndOfStream() {
            finalDrain = true
            super.playToEndOfStream()
        }
    }
}
