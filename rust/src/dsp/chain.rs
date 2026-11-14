//! Die komplette Effektkette, wie sie im Audio-Pfad von ExoPlayer läuft.
//!
//! Reihenfolge je Sample:
//!
//!   ReplayGain + EQ-Vorverstärkung  ->  EQ-Bänder  ->  Kompressor  ->  Begrenzung
//!
//! # Zwei Threads, zwei Rollen
//!
//! * UI-Thread (Dart): ändert Einstellungen, ruft [`set_active`] auf.
//! * Audio-Thread (ExoPlayer): ruft [`DspChain::process_i16_bytes`] für jeden
//!   PCM-Block auf.
//!
//! Der Audio-Thread darf nie warten, sonst knackt es. Deshalb:
//!  - Er prüft pro Block nur einen Zähler ([`VERSION`]). Das ist ein
//!    einzelner atomarer Lesezugriff.
//!  - Nur wenn sich der Zähler geändert hat, holt er sich die neue
//!    Konfiguration mit `try_read`. Ist die Sperre gerade belegt, arbeitet
//!    er einfach mit der alten weiter und versucht es beim nächsten Block.
//!  - [`ActiveConfig`] ist `Copy` mit festen Arrays: Beim Übernehmen wird
//!    nichts reserviert.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};

use crate::api::models::ReplayGainInfo;
use crate::dsp::biquad::{BandParams, Biquad, FilterKind};
use crate::dsp::compressor::{Compressor, CompressorParams};
use crate::dsp::replaygain::{gain_factor, ReplayGainMode, ReplayGainSettings};

/// Höchstzahl an EQ-Bändern. AutoEQ-Profile haben 10; 16 lässt Luft nach oben.
pub const MAX_BANDS: usize = 16;

/// Fertig aufbereitete Einstellungen für den Audio-Thread.
#[derive(Debug, Clone, Copy)]
pub struct ActiveConfig {
    pub eq_enabled: bool,
    pub preamp_db: f32,
    pub n_bands: usize,
    pub bands: [BandParams; MAX_BANDS],
    pub compressor: CompressorParams,
    pub replay_gain: ReplayGainSettings,
}

const FLAT_BAND: BandParams =
    BandParams { kind: FilterKind::Peaking, freq_hz: 1000.0, gain_db: 0.0, q: 1.0 };

impl Default for ActiveConfig {
    fn default() -> Self {
        Self {
            eq_enabled: false,
            preamp_db: 0.0,
            n_bands: 0,
            bands: [FLAT_BAND; MAX_BANDS],
            compressor: CompressorParams::default(),
            replay_gain: ReplayGainSettings {
                mode: ReplayGainMode::Off,
                preamp_db: 0.0,
                fallback_db: 0.0,
                prevent_clipping: true,
            },
        }
    }
}

impl ActiveConfig {
    /// Verändert die Kette den Ton überhaupt? Wenn nicht, kann Android den
    /// Ton per Hardware-Offload ausgeben (spart am meisten Akku).
    pub fn is_active(&self) -> bool {
        self.eq_enabled
            || self.compressor.enabled
            || self.replay_gain.mode != ReplayGainMode::Off
    }
}

static ACTIVE: LazyLock<RwLock<ActiveConfig>> =
    LazyLock::new(|| RwLock::new(ActiveConfig::default()));

/// Wird bei jeder Änderung hochgezählt (Einstellungen ODER neuer Titel).
static VERSION: AtomicU64 = AtomicU64::new(1);

/// ReplayGain-Werte des laufenden Titels als f32-Bits; NaN = nicht vorhanden.
/// Reihenfolge: Track-Gain, Album-Gain, Track-Peak, Album-Peak.
/// Atomare Werte statt einer Sperre, damit der Audio-Thread nie warten muss.
static TRACK_GAIN: [AtomicU32; 4] = [
    AtomicU32::new(NAN_BITS),
    AtomicU32::new(NAN_BITS),
    AtomicU32::new(NAN_BITS),
    AtomicU32::new(NAN_BITS),
];
const NAN_BITS: u32 = 0x7FC0_0000;

/// Neue Einstellungen aktivieren (vom UI-Thread).
pub fn set_active(cfg: ActiveConfig) {
    *ACTIVE.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    VERSION.fetch_add(1, Ordering::Release);
}

pub fn is_active() -> bool {
    ACTIVE.read().unwrap_or_else(|e| e.into_inner()).is_active()
}

/// ReplayGain-Werte des Titels setzen, der jetzt zu spielen beginnt.
pub fn set_track_gain(info: Option<&ReplayGainInfo>) {
    let bits = |v: Option<f32>| v.map(f32::to_bits).unwrap_or(NAN_BITS);
    let values = match info {
        Some(i) => [i.track_gain_db, i.album_gain_db, i.track_peak, i.album_peak],
        None => [None; 4],
    };
    for (slot, v) in TRACK_GAIN.iter().zip(values) {
        slot.store(bits(v), Ordering::Relaxed);
    }
    VERSION.fetch_add(1, Ordering::Release);
}

fn current_track_gain() -> ReplayGainInfo {
    let get = |i: usize| {
        let v = f32::from_bits(TRACK_GAIN[i].load(Ordering::Relaxed));
        (!v.is_nan()).then_some(v)
    };
    ReplayGainInfo {
        track_gain_db: get(0),
        album_gain_db: get(1),
        track_peak: get(2),
        album_peak: get(3),
    }
}

/// Zustand der Kette. Gehört dem Audio-Thread.
pub struct DspChain {
    seen_version: u64,
    cfg: ActiveConfig,
    sample_rate: u32,
    /// Ein Filter je Band UND Kanal (jeder Kanal hat seinen eigenen Verlauf).
    filters: [[Biquad; 2]; MAX_BANDS],
    compressor: Compressor,
    /// Aktuelle Verstärkung, läuft weich auf `target_gain` zu.
    gain: f32,
    target_gain: f32,
    gain_coef: f32,
}

impl Default for DspChain {
    fn default() -> Self {
        Self {
            seen_version: 0,
            cfg: ActiveConfig::default(),
            sample_rate: 0,
            filters: std::array::from_fn(|_| [Biquad::default(), Biquad::default()]),
            compressor: Compressor::default(),
            gain: 1.0,
            target_gain: 1.0,
            gain_coef: 0.0,
        }
    }
}

impl DspChain {
    /// Filterverläufe löschen (nach Vor-/Zurückspulen). Sonst würde der
    /// Nachhall der alten Stelle kurz in die neue hineinklingen.
    pub fn reset(&mut self) {
        for band in self.filters.iter_mut() {
            for f in band.iter_mut() {
                f.reset();
            }
        }
        self.compressor.reset();
    }

    /// Übernimmt neue Einstellungen, falls es welche gibt.
    fn refresh(&mut self, sample_rate: u32) {
        let version = VERSION.load(Ordering::Acquire);
        if version == self.seen_version && sample_rate == self.sample_rate {
            return;
        }
        // Sperre belegt -> mit alter Konfiguration weiter, nächster Block
        // versucht es erneut (seen_version bleibt absichtlich alt).
        let Ok(cfg) = ACTIVE.try_read().map(|g| *g) else { return };
        self.cfg = cfg;
        self.sample_rate = sample_rate;
        self.seen_version = version;

        let sr = sample_rate as f64;
        for b in 0..cfg.n_bands.min(MAX_BANDS) {
            for f in self.filters[b].iter_mut() {
                // Nur die Koeffizienten ändern, den Verlauf behalten: So
                // knackt es nicht, wenn der Nutzer am Regler zieht.
                f.set_params(cfg.bands[b], sr);
            }
        }
        self.compressor.configure(cfg.compressor, sample_rate as f32);

        let rg = gain_factor(Some(&current_track_gain()), &cfg.replay_gain);
        let preamp = if cfg.eq_enabled { 10f32.powf(cfg.preamp_db / 20.0) } else { 1.0 };
        self.target_gain = rg * preamp;
        // Lautstärkewechsel über ca. 30 ms einblenden statt hart umzuschalten.
        self.gain_coef = 1.0 - (-1.0 / (0.03 * sample_rate as f32)).exp();
    }

    /// Verarbeitet 16-Bit-PCM (verschachtelte Kanäle) an Ort und Stelle.
    ///
    /// Die Daten kommen als Bytes, weil der Puffer aus Java nicht unbedingt
    /// auf 2-Byte-Grenzen ausgerichtet ist; ein direkter `&mut [i16]` wäre
    /// dann undefiniertes Verhalten.
    pub fn process_i16_bytes(&mut self, data: &mut [u8], sample_rate: u32, channels: usize) {
        self.refresh(sample_rate);
        // Mehrkanal-Ton (5.1 usw.) lassen wir unverändert durch: Die Filter
        // sind für Stereo ausgelegt, und Musik ist praktisch immer Stereo.
        if !self.cfg.is_active() || channels == 0 || channels > 2 {
            return;
        }
        let n_bands = if self.cfg.eq_enabled { self.cfg.n_bands.min(MAX_BANDS) } else { 0 };
        let frame_bytes = channels * 2;
        let mut frame = [0f32; 2];

        for chunk in data.chunks_exact_mut(frame_bytes) {
            self.gain += (self.target_gain - self.gain) * self.gain_coef;

            for ch in 0..channels {
                let raw = i16::from_ne_bytes([chunk[ch * 2], chunk[ch * 2 + 1]]);
                let mut x = raw as f32 / 32768.0 * self.gain;
                for b in 0..n_bands {
                    x = self.filters[b][ch].process(x);
                }
                frame[ch] = x;
            }

            self.compressor.process_frame(&mut frame[..channels]);

            for ch in 0..channels {
                // Harte Begrenzung als letzte Sicherung: Ohne sie würde ein
                // zu lauter Wert beim Umwandeln "umklappen" und laut knacken.
                let out = (frame[ch].clamp(-1.0, 1.0) * 32767.0) as i16;
                chunk[ch * 2..ch * 2 + 2].copy_from_slice(&out.to_ne_bytes());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(samples: &[i16]) -> Vec<u8> {
        samples.iter().flat_map(|s| s.to_ne_bytes()).collect()
    }
    fn samples(bytes: &[u8]) -> Vec<i16> {
        bytes.chunks_exact(2).map(|b| i16::from_ne_bytes([b[0], b[1]])).collect()
    }

    /// Alle Fälle in EINEM Test, weil sie sich die globale Konfiguration
    /// teilen und `cargo test` Tests parallel ausführt.
    #[test]
    fn chain_passthrough_gain_and_clamp() {
        let mut chain = DspChain::default();

        // 1) Alles aus -> Daten bleiben bitgenau gleich.
        set_active(ActiveConfig::default());
        let original = bytes(&[1000, -1000, 20000, -20000]);
        let mut data = original.clone();
        chain.process_i16_bytes(&mut data, 48_000, 2);
        assert_eq!(data, original);

        // 2) ReplayGain -6 dB -> nach dem Einblenden halbe Amplitude.
        set_active(ActiveConfig {
            replay_gain: ReplayGainSettings {
                mode: ReplayGainMode::Track,
                preamp_db: 0.0,
                fallback_db: 0.0,
                prevent_clipping: true,
            },
            ..ActiveConfig::default()
        });
        set_track_gain(Some(&ReplayGainInfo {
            track_gain_db: Some(-6.0206),
            ..Default::default()
        }));
        let mut data = bytes(&vec![10_000i16; 48_000 * 2]);
        chain.process_i16_bytes(&mut data, 48_000, 2);
        let last = *samples(&data).last().unwrap();
        assert!((last - 5000).abs() <= 2, "war {last}");

        // 3) Starke Anhebung übersteuert nicht, sondern wird begrenzt.
        set_track_gain(Some(&ReplayGainInfo { track_gain_db: Some(20.0), ..Default::default() }));
        set_active(ActiveConfig {
            replay_gain: ReplayGainSettings {
                mode: ReplayGainMode::Track,
                preamp_db: 0.0,
                fallback_db: 0.0,
                prevent_clipping: false,
            },
            ..ActiveConfig::default()
        });
        let mut data = bytes(&vec![30_000i16; 48_000 * 2]);
        chain.process_i16_bytes(&mut data, 48_000, 2);
        assert_eq!(*samples(&data).last().unwrap(), 32767);

        // Aufräumen, damit andere Tests nicht beeinflusst werden.
        set_track_gain(None);
        set_active(ActiveConfig::default());
    }
}
