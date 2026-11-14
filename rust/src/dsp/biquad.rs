//! Biquad-Filter: der Grundbaustein des parametrischen Equalizers.
//!
//! Jedes EQ-Band ist ein Biquad mit Frequenz, Verstärkung und Güte (Q).
//! Die Formeln stammen aus dem "Audio EQ Cookbook" von Robert Bristow-Johnson,
//! dem Standard in praktisch jeder Audio-Software. AutoEQ-Profile sind genau
//! in diesen Parametern angegeben und lassen sich deshalb 1:1 übernehmen.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilterKind {
    /// Glocke: hebt/senkt ein Band um `freq` herum.
    Peaking,
    /// Hebt/senkt alles unterhalb von `freq` (Bass).
    LowShelf,
    /// Hebt/senkt alles oberhalb von `freq` (Höhen).
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandParams {
    pub kind: FilterKind,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
}

/// Normierte Koeffizienten (a0 = 1).
#[derive(Debug, Clone, Copy, Default)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    fn new(p: BandParams, sample_rate: f64) -> Self {
        let a = 10f64.powf(p.gain_db / 40.0);
        // Die Frequenz muss klar UNTER der halben Abtastrate bleiben
        // (Nyquist). Sonst wird der Filter instabil und schwingt sich auf,
        // bis nur noch Stille oder Krach herauskommt. Das passiert sonst
        // z. B. beim 16-kHz-Band des Grafik-EQ, wenn eine Datei nur
        // 22 kHz Abtastrate hat.
        let freq = p.freq_hz.clamp(10.0, 0.45 * sample_rate);
        let w0 = 2.0 * PI * freq / sample_rate;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * p.q.max(0.1));

        let (b0, b1, b2, a0, a1, a2) = match p.kind {
            FilterKind::Peaking => (
                1.0 + alpha * a,
                -2.0 * cos,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos,
                1.0 - alpha / a,
            ),
            FilterKind::LowShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos + s),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
                    a * ((a + 1.0) - (a - 1.0) * cos - s),
                    (a + 1.0) + (a - 1.0) * cos + s,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos),
                    (a + 1.0) + (a - 1.0) * cos - s,
                )
            }
            FilterKind::HighShelf => {
                let s = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos + s),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
                    a * ((a + 1.0) + (a - 1.0) * cos - s),
                    (a + 1.0) - (a - 1.0) * cos + s,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos),
                    (a + 1.0) - (a - 1.0) * cos - s,
                )
            }
        };
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }
}

/// Verstärkung (dB) eines Bandes bei der Frequenz `freq_hz`.
///
/// Wird nur für die EQ-Kurve in der Oberfläche gebraucht. Die Formel wertet
/// die Übertragungsfunktion direkt aus, statt ein Testsignal durchzuschicken:
/// Das ist exakt und so schnell, dass die Kurve beim Ziehen eines Reglers
/// flüssig mitläuft.
pub fn magnitude_db(params: BandParams, sample_rate: f64, freq_hz: f64) -> f64 {
    let c = Coeffs::new(params, sample_rate);
    let w = 2.0 * PI * freq_hz / sample_rate;
    let (cos1, cos2) = (w.cos(), (2.0 * w).cos());
    let (sin1, sin2) = (w.sin(), (2.0 * w).sin());
    // H(e^jw) = (b0 + b1 e^-jw + b2 e^-2jw) / (1 + a1 e^-jw + a2 e^-2jw)
    let num_re = c.b0 + c.b1 * cos1 + c.b2 * cos2;
    let num_im = -(c.b1 * sin1 + c.b2 * sin2);
    let den_re = 1.0 + c.a1 * cos1 + c.a2 * cos2;
    let den_im = -(c.a1 * sin1 + c.a2 * sin2);
    let mag2 = (num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im);
    10.0 * mag2.max(1e-20).log10()
}

/// Ein Filter für EINEN Kanal. Für Stereo braucht man zwei davon, weil jeder
/// Kanal seinen eigenen Verlauf (z1, z2) hat.
#[derive(Debug, Clone, Default)]
pub struct Biquad {
    c: Coeffs,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn new(params: BandParams, sample_rate: f64) -> Self {
        Self { c: Coeffs::new(params, sample_rate), z1: 0.0, z2: 0.0 }
    }

    /// Neue Parameter übernehmen, OHNE den Verlauf zu löschen. So knackt es
    /// nicht, wenn der Nutzer während der Wiedergabe am Regler zieht.
    pub fn set_params(&mut self, params: BandParams, sample_rate: f64) {
        self.c = Coeffs::new(params, sample_rate);
    }

    /// Verlauf löschen (z. B. nach dem Spulen).
    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// "Transposed Direct Form II": braucht nur zwei Zustandswerte und ist
    /// numerisch die stabilste der üblichen Varianten.
    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let x = x as f64;
        let y = self.c.b0 * x + self.z1;
        self.z1 = self.c.b1 * x - self.c.a1 * y + self.z2;
        self.z2 = self.c.b2 * x - self.c.a2 * y;
        // Letzte Sicherung: Ist das Ergebnis keine gültige Zahl mehr, würde
        // der Fehler im Verlauf (z1/z2) hängen bleiben und jeden weiteren
        // Ton zerstören. Dann den Filter zurücksetzen und diesen einen
        // Wert unverändert durchlassen.
        if !y.is_finite() {
            self.reset();
            return x as f32;
        }
        y as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Schickt einen Sinus durch den Filter und misst die Verstärkung in dB.
    fn gain_at(p: BandParams, freq: f64) -> f64 {
        let sr = 48_000.0;
        let mut f = Biquad::new(p, sr);
        let mut peak: f32 = 0.0;
        for i in 0..48_000 {
            let x = (2.0 * PI * freq * i as f64 / sr).sin() as f32;
            let y = f.process(x);
            if i > 24_000 {
                // erst nach dem Einschwingen messen
                peak = peak.max(y.abs());
            }
        }
        20.0 * (peak as f64).log10()
    }

    #[test]
    fn peaking_boosts_center_frequency() {
        let p = BandParams { kind: FilterKind::Peaking, freq_hz: 1000.0, gain_db: 6.0, q: 1.0 };
        assert!((gain_at(p, 1000.0) - 6.0).abs() < 0.2);
        // Weit weg vom Band: praktisch unverändert.
        assert!(gain_at(p, 100.0).abs() < 0.5);
    }

    #[test]
    fn computed_curve_matches_measured_filter() {
        // Die berechnete Kurve (für die Anzeige) muss zum echten Filter passen.
        let p = BandParams { kind: FilterKind::HighShelf, freq_hz: 4000.0, gain_db: -5.0, q: 0.7 };
        for f in [200.0, 4000.0, 12_000.0] {
            assert!((magnitude_db(p, 48_000.0, f) - gain_at(p, f)).abs() < 0.1);
        }
    }

    #[test]
    fn band_above_nyquist_stays_stable() {
        // 16-kHz-Band bei 22,05 kHz Abtastrate: früher instabil.
        let p = BandParams { kind: FilterKind::Peaking, freq_hz: 16_000.0, gain_db: 12.0, q: 1.41 };
        let mut f = Biquad::new(p, 22_050.0);
        let mut peak = 0f32;
        for i in 0..44_100 {
            let x = (2.0 * PI * 1000.0 * i as f64 / 22_050.0).sin() as f32;
            let y = f.process(x);
            assert!(y.is_finite());
            peak = peak.max(y.abs());
        }
        // Bleibt in einem vernünftigen Rahmen (höchstens die 12 dB Anhebung).
        assert!(peak < 5.0, "Filter schwingt sich auf: {peak}");
    }

    #[test]
    fn low_shelf_boosts_bass_only() {
        let p = BandParams { kind: FilterKind::LowShelf, freq_hz: 200.0, gain_db: 4.0, q: 0.707 };
        assert!((gain_at(p, 30.0) - 4.0).abs() < 0.3);
        assert!(gain_at(p, 5000.0).abs() < 0.3);
    }
}
