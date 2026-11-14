//! ReplayGain: gleiche Lautstärke über alle Titel und Alben hinweg.
//!
//! Die Werte (Gain in dB, Peak) stehen in den Tags der Dateien und kommen
//! vom Server. Wir berechnen daraus nur einen Faktor, mit dem jedes Sample
//! multipliziert wird. Das ist so billig, dass es keinen messbaren Akku kostet.

use crate::api::models::ReplayGainInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayGainMode {
    Off,
    /// Jeder Titel gleich laut.
    Track,
    /// Ganze Alben gleich laut. Lautstärkeunterschiede INNERHALB des Albums
    /// bleiben erhalten (wichtig z. B. bei leisen Intros).
    Album,
}

#[derive(Debug, Clone, Copy)]
pub struct ReplayGainSettings {
    pub mode: ReplayGainMode,
    /// Vorverstärkung (Preamp) für Titel MIT ReplayGain-Tags.
    pub preamp_db: f32,
    /// Verstärkung für Titel OHNE Tags (meist leicht negativ, weil
    /// ungetaggte Titel typischerweise lauter gemastert sind).
    pub fallback_db: f32,
    /// Schutz vor Übersteuern: Faktor so begrenzen, dass der lauteste Punkt
    /// (Peak) nicht über 1.0 geht.
    pub prevent_clipping: bool,
}

/// Linearer Faktor (1.0 = unverändert) für einen Titel.
pub fn gain_factor(info: Option<&ReplayGainInfo>, s: &ReplayGainSettings) -> f32 {
    if s.mode == ReplayGainMode::Off {
        return 1.0;
    }
    // Fehlt ein Album-Wert, nehmen wir den Track-Wert und umgekehrt.
    let (gain, peak) = match (s.mode, info) {
        (_, None) => (None, None),
        (ReplayGainMode::Album, Some(i)) => {
            (i.album_gain_db.or(i.track_gain_db), i.album_peak.or(i.track_peak))
        }
        (_, Some(i)) => (i.track_gain_db.or(i.album_gain_db), i.track_peak.or(i.album_peak)),
    };
    let db = match gain {
        Some(g) => g + s.preamp_db,
        None => s.fallback_db,
    };
    let mut factor = 10f32.powf(db / 20.0);
    if s.prevent_clipping {
        if let Some(p) = peak.filter(|p| *p > 0.0) {
            factor = factor.min(1.0 / p);
        }
    }
    factor
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: ReplayGainMode) -> ReplayGainSettings {
        ReplayGainSettings { mode, preamp_db: 0.0, fallback_db: -6.0, prevent_clipping: true }
    }

    #[test]
    fn minus_six_db_halves_amplitude() {
        let i = ReplayGainInfo { track_gain_db: Some(-6.0206), ..Default::default() };
        assert!((gain_factor(Some(&i), &settings(ReplayGainMode::Track)) - 0.5).abs() < 0.001);
    }

    #[test]
    fn clipping_protection_limits_boost() {
        // +6 dB wäre ×2, aber Peak 0.8 erlaubt höchstens ×1.25.
        let i = ReplayGainInfo {
            track_gain_db: Some(6.0),
            track_peak: Some(0.8),
            ..Default::default()
        };
        assert!((gain_factor(Some(&i), &settings(ReplayGainMode::Track)) - 1.25).abs() < 0.001);
    }

    #[test]
    fn untagged_track_uses_fallback() {
        let f = gain_factor(None, &settings(ReplayGainMode::Album));
        assert!((f - 0.501).abs() < 0.01);
    }
}
