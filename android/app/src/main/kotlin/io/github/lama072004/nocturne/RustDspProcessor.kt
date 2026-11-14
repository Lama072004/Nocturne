package io.github.lama072004.nocturne

import androidx.annotation.OptIn
import androidx.media3.common.C
import androidx.media3.common.audio.AudioProcessor.AudioFormat
import androidx.media3.common.audio.BaseAudioProcessor
import androidx.media3.common.util.UnstableApi
import java.nio.ByteBuffer

/**
 * Hängt die Rust-Effektkette (ReplayGain, Equalizer, Kompressor) in den
 * Audio-Pfad von ExoPlayer.
 *
 * ExoPlayer ruft [queueInput] für jeden dekodierten PCM-Block auf, kurz
 * bevor er an den Lautsprecher geht. Wir kopieren den Block in unseren
 * Ausgabepuffer und lassen Rust ihn dort an Ort und Stelle bearbeiten.
 *
 * WARUM ist der Prozessor IMMER aktiv, auch wenn alle Effekte aus sind?
 * ExoPlayer legt beim Start eines Titels fest, welche Prozessoren mitlaufen.
 * Wäre unserer dann "aus", würde ein später eingeschalteter Equalizer erst
 * beim nächsten Titel hörbar. So wirkt jede Änderung sofort. Bei
 * ausgeschalteten Effekten kostet der Durchlauf nur eine Speicherkopie.
 */
@OptIn(UnstableApi::class)
class RustDspProcessor : BaseAudioProcessor() {

    override fun onConfigure(inputAudioFormat: AudioFormat): AudioFormat {
        // ExoPlayer wandelt vor diesem Prozessor alles in 16-Bit-PCM um. Für
        // genau dieses Format ist die Rust-Kette geschrieben. Bei jedem
        // anderen Format melden wir "nicht zuständig" (NOT_SET): Der Ton
        // läuft dann unverändert an uns vorbei.
        if (inputAudioFormat.encoding != C.ENCODING_PCM_16BIT) return AudioFormat.NOT_SET
        return inputAudioFormat
    }

    override fun queueInput(inputBuffer: ByteBuffer) {
        val size = inputBuffer.remaining()
        if (size == 0) return

        // Der Puffer von replaceOutputBuffer ist "direkt" (liegt außerhalb
        // des Java-Heaps). Nur darauf kann Rust ohne weitere Kopie zugreifen.
        val out = replaceOutputBuffer(size)
        val start = out.position()
        out.put(inputBuffer)

        if (NativeDsp.active) {
            NativeDsp.nativeProcess(
                out,
                start,
                size,
                inputAudioFormat.sampleRate,
                inputAudioFormat.channelCount,
            )
        }
        // Auf "Lesen" umschalten: ExoPlayer holt sich die Daten jetzt ab.
        out.flip()
    }

    /** Nach dem Spulen: alte Filterverläufe verwerfen. */
    override fun onFlush() {
        if (NativeDsp.available) NativeDsp.nativeReset()
    }
}
