package io.github.lama072004.nocturne

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Prüft den Rechenkern der Überblendung ohne Gerät.
 *
 * WARUM so wichtig? Ob eine Überblendung "gut klingt", lässt sich im
 * Emulator nicht hören. Was sich aber hart prüfen lässt: Es geht kein Ton
 * verloren, nichts wird doppelt ausgegeben, die Längen stimmen, und an den
 * Rändern der Überblendung gibt es keinen Sprung.
 */
class CrossfadeBufferTest {

    private val rate = 1000 // 1000 Frames = 1 Sekunde -> leicht zu rechnen
    private val frameBytes = 4 // Stereo, 16 Bit

    /** [frames] Stereo-Frames, alle Samples mit demselben Wert. */
    private fun block(frames: Int, value: Short): ByteBuffer {
        val b = ByteBuffer.allocate(frames * frameBytes).order(ByteOrder.LITTLE_ENDIAN)
        repeat(frames * 2) { b.putShort(value) }
        b.flip()
        return b
    }

    /** Schiebt [input] hinein und sammelt, was sofort herauskommt. */
    private fun feed(buf: CrossfadeBuffer, input: ByteBuffer, out: MutableList<Short>) {
        buf.push(input)
        drain(buf, buf.available(), out)
    }

    private fun drain(buf: CrossfadeBuffer, count: Int, out: MutableList<Short>) {
        if (count == 0) return
        val dst = ByteBuffer.allocate(count).order(ByteOrder.LITTLE_ENDIAN)
        buf.pop(dst, count)
        dst.flip()
        // Nur den linken Kanal merken; der rechte muss gleich sein.
        while (dst.remaining() >= frameBytes) {
            val left = dst.getShort()
            val right = dst.getShort()
            assertEquals("Links und rechts müssen gleich bleiben", left, right)
            out += left
        }
    }

    private fun newBuffer(delayMs: Int) = CrossfadeBuffer().apply {
        configure(rate, 2)
        setDelayMs(delayMs)
    }

    @Test
    fun ohneUeberblendungLaeuftAllesUnveraendertDurch() {
        val buf = newBuffer(0)
        assertTrue(buf.isPassThrough)
        val out = mutableListOf<Short>()
        feed(buf, block(300, 1234), out)
        assertEquals(300, out.size)
        assertTrue(out.all { it == 1234.toShort() })
    }

    @Test
    fun uebergangIstUmDieUeberblendungKuerzerUndOhneSprung() {
        val buf = newBuffer(1000) // 1 s = 1000 Frames
        val out = mutableListOf<Short>()

        // Titel A: 5000 Frames mit +1000 (in ungleichen Blöcken, wie echt).
        feed(buf, block(1700, 1000), out)
        feed(buf, block(3300, 1000), out)
        // Bis hierher darf nur herausgekommen sein, was über 1 s hinausgeht.
        assertEquals(4000, out.size)

        // Titelgrenze: Die letzte Sekunde von A wird zum Ausklang.
        assertEquals(1000, buf.markBoundary())

        // Titel B: 5000 Frames mit -1000.
        feed(buf, block(2500, -1000), out)
        feed(buf, block(2500, -1000), out)
        // Ende der Wiedergabe: Rest ausgeben.
        drain(buf, buf.pending(), out)

        // Gesamtlänge = A + B - Überblendung.
        assertEquals(5000 + 5000 - 1000, out.size)

        // Vor der Überblendung: reines A.
        assertTrue(out.subList(0, 4000).all { it == 1000.toShort() })
        // Nach der Überblendung: reines B.
        assertTrue(out.subList(5000, 9000).all { it == (-1000).toShort() })

        // In der Überblendung: beginnt bei A, endet bei B, fällt stetig.
        val fade = out.subList(4000, 5000)
        assertTrue("Beginn nahe A", fade.first() >= 990)
        assertTrue("Ende nahe B", fade.last() <= -990)
        for (i in 1 until fade.size) {
            assertTrue("Verlauf muss fallen", fade[i] <= fade[i - 1])
            // Kein Sprung: Nachbarn unterscheiden sich nur wenig.
            assertTrue("Kein Knacken", fade[i - 1] - fade[i] < 10)
        }
    }

    @Test
    fun kurzerAusklangWirdNurSoLangGemischtWieErIst() {
        val buf = newBuffer(1000)
        val out = mutableListOf<Short>()
        // Nur 300 Frames von A (z. B. kurz vor dem Ende hingespult). Die
        // Verzögerung baut sich erst auf: Die Hälfte kommt sofort heraus.
        feed(buf, block(300, 1000), out)
        assertEquals(150, out.size)
        assertEquals(150, buf.markBoundary())
        feed(buf, block(2000, -1000), out)
        drain(buf, buf.pending(), out)
        // 150 Frames A unverändert, dann B – in dessen erste 150 Frames der
        // Ausklang gemischt ist.
        assertEquals(150 + 2000, out.size)
        assertTrue(out.subList(0, 150).all { it == 1000.toShort() })
        assertTrue(out[150] >= 990)
        assertTrue(out.subList(300, 2150).all { it == (-1000).toShort() })
    }

    @Test
    fun leerenVerwirftAuchDenAusklang() {
        val buf = newBuffer(1000)
        val out = mutableListOf<Short>()
        feed(buf, block(1500, 1000), out)
        assertEquals(750, out.size)
        buf.markBoundary()
        buf.clear() // Nutzer spult
        feed(buf, block(1500, -1000), out)
        drain(buf, buf.pending(), out)
        // 750 Frames A (vor dem Leeren) + 1500 Frames B, nichts gemischt.
        assertEquals(750 + 1500, out.size)
        assertTrue(out.subList(750, 2250).all { it == (-1000).toShort() })
    }

    @Test
    fun anderesFormatVerwirftDasGepufferte() {
        val buf = newBuffer(1000)
        val out = mutableListOf<Short>()
        feed(buf, block(800, 1000), out)
        buf.markBoundary()
        // Nächster Titel hat eine andere Abtastrate.
        buf.configure(2000, 2)
        buf.setDelayMs(0)
        assertTrue(buf.isPassThrough)
    }

    @Test
    fun tonKommtSofortUndVerzoegerungWaechstAllmaehlich() {
        // Wichtig gegen Aussetzer: Jeder Block liefert sofort Nachschub
        // (mindestens die Hälfte), nie "erst einmal gar nichts".
        val buf = newBuffer(1000)
        val out = mutableListOf<Short>()
        var before = 0
        repeat(10) {
            feed(buf, block(200, 1000), out)
            assertTrue("Jeder Block muss Ton liefern", out.size - before >= 100)
            before = out.size
        }
        // Nach 2000 Frames Eingang ist die volle Verzögerung (1000) erreicht.
        assertEquals(1000, out.size)
        feed(buf, block(200, 1000), out)
        assertEquals(1200, out.size)
    }

    @Test
    fun abschaltenGibtDenRestNochAus() {
        val buf = newBuffer(1000)
        val out = mutableListOf<Short>()
        feed(buf, block(1500, 1000), out)
        assertEquals(750, out.size)
        buf.setDelayMs(0) // Nutzer schaltet Crossfade aus
        feed(buf, block(100, 1000), out)
        // Alles Zurückgehaltene kommt jetzt heraus – nichts geht verloren.
        assertEquals(1600, out.size)
        assertTrue(buf.isPassThrough)
    }
}
