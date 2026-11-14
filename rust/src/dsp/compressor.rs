//! Kompressor: gleicht Lautstärkeunterschiede zwischen leisen und lauten
//! Passagen aus.
//!
//! Funktionsweise in einem Satz: Alles, was lauter als die Schwelle
//! (`threshold_db`) ist, wird um den Faktor `ratio` abgeschwächt; danach hebt
//! `makeup_db` das Ganze wieder an. Leises wird so im Verhältnis lauter.
//!
//! Der Pegel wird über beide Kanäle GEMEINSAM gemessen ("stereo-linked").
//! Würde jeder Kanal einzeln geregelt, wanderte das Stereobild hin und her,
//! sobald nur eine Seite laut ist.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorParams {
    pub enabled: bool,
    /// Ab diesem Pegel (dBFS, also negativ) wird abgeschwächt.
    pub threshold_db: f32,
    /// 4.0 bedeutet: 4 dB über der Schwelle werden zu 1 dB.
    pub ratio: f32,
    /// Wie schnell der Kompressor zupackt.
    pub attack_ms: f32,
    /// Wie schnell er wieder loslässt.
    pub release_ms: f32,
    /// Ausgleichs-Verstärkung nach der Kompression.
    pub makeup_db: f32,
}

impl Default for CompressorParams {
    fn default() -> Self {
        // Sanfte Voreinstellung für Musik: greift erst bei lauten Stellen
        // und pumpt nicht hörbar.
        Self {
            enabled: false,
            threshold_db: -18.0,
            ratio: 3.0,
            attack_ms: 10.0,
            release_ms: 150.0,
            makeup_db: 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Compressor {
    params: CompressorParams,
    attack_coef: f32,
    release_coef: f32,
    makeup: f32,
    /// Aktuelle Abschwächung in dB (geglättet). 0 = keine.
    reduction_db: f32,
}

impl Compressor {
    pub fn configure(&mut self, params: CompressorParams, sample_rate: f32) {
        self.params = params;
        // Übliche Formel für eine Glättung mit vorgegebener Zeitkonstante.
        // `max(0.1)` verhindert eine Division durch null bei 0 ms.
        let coef = |ms: f32| (-1.0 / (ms.max(0.1) * 0.001 * sample_rate)).exp();
        self.attack_coef = coef(params.attack_ms);
        self.release_coef = coef(params.release_ms);
        self.makeup = 10f32.powf(params.makeup_db / 20.0);
    }

    pub fn reset(&mut self) {
        self.reduction_db = 0.0;
    }

    /// Verarbeitet einen Frame (ein Sample pro Kanal) an Ort und Stelle.
    #[inline]
    pub fn process_frame(&mut self, frame: &mut [f32]) {
        if !self.params.enabled {
            return;
        }
        let peak = frame.iter().fold(0f32, |m, s| m.max(s.abs()));
        // +1e-9: log10(0) wäre minus unendlich.
        let level_db = 20.0 * (peak + 1e-9).log10();
        let over = level_db - self.params.threshold_db;
        let target = if over > 0.0 { over * (1.0 - 1.0 / self.params.ratio.max(1.0)) } else { 0.0 };

        // Mehr Abschwächung nötig -> schnell (attack); weniger -> langsam (release).
        let coef = if target > self.reduction_db { self.attack_coef } else { self.release_coef };
        self.reduction_db = target + coef * (self.reduction_db - target);

        let gain = 10f32.powf(-self.reduction_db / 20.0) * self.makeup;
        for s in frame.iter_mut() {
            *s *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gibt den Pegel (dB) eines konstanten Signals nach dem Einschwingen zurück.
    fn settled_level(input_db: f32, p: CompressorParams) -> f32 {
        let mut c = Compressor::default();
        c.configure(p, 48_000.0);
        let x = 10f32.powf(input_db / 20.0);
        let mut out = 0.0;
        for _ in 0..48_000 {
            let mut frame = [x, x];
            c.process_frame(&mut frame);
            out = frame[0];
        }
        20.0 * out.log10()
    }

    fn params() -> CompressorParams {
        CompressorParams {
            enabled: true,
            threshold_db: -20.0,
            ratio: 4.0,
            attack_ms: 5.0,
            release_ms: 50.0,
            makeup_db: 0.0,
        }
    }

    #[test]
    fn quiet_signal_is_untouched() {
        assert!((settled_level(-30.0, params()) - -30.0).abs() < 0.05);
    }

    #[test]
    fn loud_signal_is_reduced_by_ratio() {
        // 12 dB über der Schwelle, Ratio 4 -> bleiben 3 dB über der Schwelle.
        assert!((settled_level(-8.0, params()) - -17.0).abs() < 0.1);
    }

    #[test]
    fn disabled_compressor_does_nothing() {
        let p = CompressorParams { enabled: false, ..params() };
        assert!((settled_level(-8.0, p) - -8.0).abs() < 0.01);
    }
}
