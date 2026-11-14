//! Klang-Einstellungen: Equalizer, Kompressor, ReplayGain.
//!
//! Dieses Modul ist die Verwaltung; gerechnet wird in `crate::dsp`.
//!  - speichert die Einstellungen (`dsp.json`)
//!  - wählt das passende EQ-Profil für das aktuelle Ausgabegerät
//!  - übersetzt alles in eine `ActiveConfig` für den Audio-Thread
//!  - liest AutoEQ-Profile ein
//!
//! Eigene Datei statt Teil von `settings.json`: Die Klang-Einstellungen
//! ändern sich beim Ziehen eines Reglers viele Male pro Sekunde. So wird
//! dabei nicht jedes Mal die komplette App-Konfiguration neu geschrieben.

use std::path::PathBuf;
use std::sync::{LazyLock, RwLock};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::{runtime, storage};
use crate::dsp::biquad::{self, BandParams, FilterKind};
use crate::dsp::chain::{self, ActiveConfig, MAX_BANDS};
use crate::dsp::compressor::CompressorParams;
use crate::dsp::replaygain::{ReplayGainMode, ReplayGainSettings};

// ---------------------------------------------------------------------------
// Typen
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqFilterKind {
    /// Glocke um eine Frequenz herum.
    Peaking,
    /// Alles unterhalb der Frequenz (Bass).
    LowShelf,
    /// Alles oberhalb der Frequenz (Höhen).
    HighShelf,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EqBand {
    pub kind: EqFilterKind,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EqMode {
    /// Frei einstellbare Bänder (Frequenz, Gain, Q, Typ).
    Parametric,
    /// Einfacher Modus: 10 feste Frequenzen mit je einem Schieberegler.
    Graphic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqProfile {
    pub enabled: bool,
    pub mode: EqMode,
    /// Vorverstärkung. Meist negativ, damit angehobene Bänder nicht übersteuern.
    pub preamp_db: f32,
    /// Bänder des parametrischen Modus.
    pub bands: Vec<EqBand>,
    /// Gains des 10-Band-Modus (immer 10 Werte).
    pub graphic_gains_db: Vec<f32>,
}

/// Feste Frequenzen des 10-Band-Modus (Oktavabstand, wie bei jedem
/// klassischen Grafik-EQ).
const GRAPHIC_FREQS: [f32; 10] =
    [31.0, 62.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
/// Q für Oktavbänder: Die Bänder überlappen sich dann sauber.
const GRAPHIC_Q: f32 = 1.41;

impl Default for EqProfile {
    fn default() -> Self {
        let flat = |freq_hz: f32| EqBand { kind: EqFilterKind::Peaking, freq_hz, gain_db: 0.0, q: 1.0 };
        Self {
            enabled: false,
            mode: EqMode::Graphic,
            preamp_db: 0.0,
            bands: vec![flat(60.0), flat(250.0), flat(1000.0), flat(4000.0), flat(12000.0)],
            graphic_gains_db: vec![0.0; 10],
        }
    }
}

impl EqProfile {
    /// Die tatsächlich wirksamen Bänder, je nach Modus.
    fn effective_bands(&self) -> Vec<EqBand> {
        match self.mode {
            EqMode::Parametric => self.bands.iter().copied().take(MAX_BANDS).collect(),
            EqMode::Graphic => GRAPHIC_FREQS
                .iter()
                .zip(self.graphic_gains_db.iter().chain(std::iter::repeat(&0.0)))
                .map(|(f, g)| EqBand {
                    kind: EqFilterKind::Peaking,
                    freq_hz: *f,
                    gain_db: *g,
                    q: GRAPHIC_Q,
                })
                .collect(),
        }
    }
}

fn to_params(b: &EqBand) -> BandParams {
    BandParams {
        kind: match b.kind {
            EqFilterKind::Peaking => FilterKind::Peaking,
            EqFilterKind::LowShelf => FilterKind::LowShelf,
            EqFilterKind::HighShelf => FilterKind::HighShelf,
        },
        // Werte begrenzen: Eine Frequenz von 0 oder ein Q von 0 würde in den
        // Filterformeln durch null teilen.
        freq_hz: (b.freq_hz as f64).clamp(10.0, 22_000.0),
        gain_db: (b.gain_db as f64).clamp(-24.0, 24.0),
        q: (b.q as f64).clamp(0.1, 20.0),
    }
}

/// Eigenes EQ-Profil für ein bestimmtes Ausgabegerät.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceProfile {
    /// Stabiler Schlüssel, z. B. "bt:WH-1000XM4", "wired", "speaker".
    pub device_key: String,
    /// Anzeigename.
    pub device_name: String,
    pub profile: EqProfile,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CompressorConfig {
    pub enabled: bool,
    pub threshold_db: f32,
    pub ratio: f32,
    pub attack_ms: f32,
    pub release_ms: f32,
    pub makeup_db: f32,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        let d = CompressorParams::default();
        Self {
            enabled: d.enabled,
            threshold_db: d.threshold_db,
            ratio: d.ratio,
            attack_ms: d.attack_ms,
            release_ms: d.release_ms,
            makeup_db: d.makeup_db,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayGainChoice {
    Off,
    /// Jeder Titel gleich laut.
    Track,
    /// Alben als Ganzes gleich laut.
    Album,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ReplayGainConfig {
    pub mode: ReplayGainChoice,
    /// Vorverstärkung für Titel mit ReplayGain-Tags.
    pub preamp_db: f32,
    /// Verstärkung für Titel ohne Tags.
    pub fallback_db: f32,
    /// Schutz vor Übersteuerung.
    pub prevent_clipping: bool,
}

impl Default for ReplayGainConfig {
    fn default() -> Self {
        Self { mode: ReplayGainChoice::Off, preamp_db: 0.0, fallback_db: -6.0, prevent_clipping: true }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DspConfig {
    /// Standard-EQ, gilt für alle Geräte ohne eigenes Profil.
    pub eq: EqProfile,
    pub device_profiles: Vec<DeviceProfile>,
    pub compressor: CompressorConfig,
    pub replay_gain: ReplayGainConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutputDevice {
    pub key: String,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Zustand
// ---------------------------------------------------------------------------

static CONFIG: LazyLock<RwLock<DspConfig>> = LazyLock::new(|| RwLock::new(DspConfig::default()));

/// Aktuelles Ausgabegerät. Android meldet es über die JNI-Brücke.
static DEVICE: RwLock<Option<OutputDevice>> = RwLock::new(None);

fn config_path() -> Result<PathBuf> {
    Ok(storage::data_dir()?.join("dsp.json"))
}

pub(crate) fn load_from_disk() -> Result<()> {
    let cfg: DspConfig = storage::read_json(&config_path()?)?.unwrap_or_default();
    *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = cfg;
    apply();
    Ok(())
}

/// Das EQ-Profil, das gerade gilt: das des aktuellen Geräts, sonst der Standard.
fn effective_eq(cfg: &DspConfig) -> EqProfile {
    let device = DEVICE.read().unwrap_or_else(|e| e.into_inner());
    device
        .as_ref()
        .and_then(|d| cfg.device_profiles.iter().find(|p| p.device_key == d.key))
        .map(|p| p.profile.clone())
        .unwrap_or_else(|| cfg.eq.clone())
}

/// Baut aus den Einstellungen die Konfiguration für den Audio-Thread.
fn apply() {
    let cfg = CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone();
    let eq = effective_eq(&cfg);
    let bands = eq.effective_bands();

    let mut active = ActiveConfig {
        eq_enabled: eq.enabled,
        preamp_db: eq.preamp_db.clamp(-24.0, 12.0),
        n_bands: bands.len().min(MAX_BANDS),
        compressor: CompressorParams {
            enabled: cfg.compressor.enabled,
            threshold_db: cfg.compressor.threshold_db,
            ratio: cfg.compressor.ratio,
            attack_ms: cfg.compressor.attack_ms,
            release_ms: cfg.compressor.release_ms,
            makeup_db: cfg.compressor.makeup_db,
        },
        replay_gain: ReplayGainSettings {
            mode: match cfg.replay_gain.mode {
                ReplayGainChoice::Off => ReplayGainMode::Off,
                ReplayGainChoice::Track => ReplayGainMode::Track,
                ReplayGainChoice::Album => ReplayGainMode::Album,
            },
            preamp_db: cfg.replay_gain.preamp_db,
            fallback_db: cfg.replay_gain.fallback_db,
            prevent_clipping: cfg.replay_gain.prevent_clipping,
        },
        ..ActiveConfig::default()
    };
    for (slot, band) in active.bands.iter_mut().zip(bands.iter()) {
        *slot = to_params(band);
    }
    chain::set_active(active);
}

/// Android meldet ein neues Ausgabegerät (Kopfhörer verbunden usw.).
// Wird nur über JNI (Android) aufgerufen; auf dem PC wäre es sonst "unbenutzt".
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn set_output_device(key: String, name: String) {
    *DEVICE.write().unwrap_or_else(|e| e.into_inner()) = Some(OutputDevice { key, name });
    // Das Gerät kann ein eigenes Profil haben -> Kette neu aufbauen.
    apply();
}

// ---------------------------------------------------------------------------
// Schnittstelle für Dart
// ---------------------------------------------------------------------------

#[flutter_rust_bridge::frb(sync)]
pub fn get_dsp_config() -> DspConfig {
    CONFIG.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Übernimmt neue Klang-Einstellungen: sofort hörbar und gespeichert.
pub fn save_dsp_config(config: DspConfig) -> Result<()> {
    *CONFIG.write().unwrap_or_else(|e| e.into_inner()) = config.clone();
    // Erst aktivieren, dann speichern: Der Regler soll ohne Verzögerung
    // durch den Schreibzugriff hörbar reagieren.
    apply();
    storage::write_json(&config_path()?, &config)
}

/// Verändert die Effektkette gerade den Ton? (Wenn nicht, darf Android
/// stromsparendes Hardware-Offload nutzen.)
#[flutter_rust_bridge::frb(sync)]
pub fn dsp_is_active() -> bool {
    chain::is_active()
}

#[flutter_rust_bridge::frb(sync)]
pub fn current_output_device() -> Option<OutputDevice> {
    DEVICE.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Frequenzen des 10-Band-Modus (für die Beschriftung der Regler).
#[flutter_rust_bridge::frb(sync)]
pub fn graphic_frequencies() -> Vec<f32> {
    GRAPHIC_FREQS.to_vec()
}

/// Ein Punkt der EQ-Kurve.
#[derive(Debug, Clone, Copy)]
pub struct CurvePoint {
    pub freq_hz: f32,
    pub gain_db: f32,
}

/// Berechnet die Frequenzkurve eines Profils für die Anzeige:
/// `points` Stützstellen von 20 Hz bis 20 kHz, logarithmisch verteilt
/// (so, wie das Ohr Tonhöhen wahrnimmt). Die Vorverstärkung ist eingerechnet.
#[flutter_rust_bridge::frb(sync)]
pub fn eq_response(profile: EqProfile, points: u32) -> Vec<CurvePoint> {
    let bands: Vec<BandParams> = profile.effective_bands().iter().map(to_params).collect();
    let n = points.clamp(2, 512);
    (0..n)
        .map(|i| {
            let t = i as f64 / (n - 1) as f64;
            let freq = 20.0 * (1000.0f64).powf(t); // 20 Hz … 20 kHz
            let gain: f64 =
                bands.iter().map(|b| biquad::magnitude_db(*b, 48_000.0, freq)).sum::<f64>()
                    + profile.preamp_db as f64;
            CurvePoint { freq_hz: freq as f32, gain_db: gain as f32 }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// AutoEQ
// ---------------------------------------------------------------------------

/// Liest ein AutoEQ-Profil im "ParametricEQ.txt"-Format, z. B.:
///
/// ```text
/// Preamp: -6.3 dB
/// Filter 1: ON LSC Fc 105 Hz Gain 6.5 dB Q 0.70
/// Filter 2: ON PK Fc 125 Hz Gain -2.7 dB Q 0.55
/// ```
///
/// Das ist dasselbe Format, das auch "Equalizer APO" und "Peace" verwenden.
/// Profile aus diesen Programmen lassen sich also genauso einfügen.
pub fn parse_autoeq(text: String) -> Result<EqProfile> {
    let mut preamp = 0.0f32;
    let mut bands = Vec::new();

    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        // Wert, der direkt auf ein Schlüsselwort folgt ("Fc 105" -> 105).
        let after = |key: &str| -> Option<f32> {
            tokens.iter().position(|t| t.eq_ignore_ascii_case(key))
                .and_then(|i| tokens.get(i + 1))
                .and_then(|v| v.parse().ok())
        };

        if line.trim_start().to_ascii_lowercase().starts_with("preamp") {
            preamp = tokens.get(1).and_then(|v| v.parse().ok()).unwrap_or(0.0);
            continue;
        }
        if !line.trim_start().to_ascii_lowercase().starts_with("filter") {
            continue;
        }
        // Ausgeschaltete Filter ("OFF") überspringen.
        if tokens.iter().any(|t| t.eq_ignore_ascii_case("OFF")) {
            continue;
        }
        let kind = if tokens.iter().any(|t| matches!(t.to_ascii_uppercase().as_str(), "LSC" | "LS")) {
            EqFilterKind::LowShelf
        } else if tokens.iter().any(|t| matches!(t.to_ascii_uppercase().as_str(), "HSC" | "HS")) {
            EqFilterKind::HighShelf
        } else if tokens.iter().any(|t| t.eq_ignore_ascii_case("PK")) {
            EqFilterKind::Peaking
        } else {
            // Unbekannter Filtertyp (Hochpass usw.): auslassen statt raten.
            continue;
        };
        let (Some(freq_hz), Some(gain_db)) = (after("Fc"), after("Gain")) else { continue };
        bands.push(EqBand { kind, freq_hz, gain_db, q: after("Q").unwrap_or(0.7) });
    }

    if bands.is_empty() {
        bail!("Kein gültiges AutoEQ-Profil (keine Filter gefunden)");
    }
    bands.truncate(MAX_BANDS);
    Ok(EqProfile {
        enabled: true,
        mode: EqMode::Parametric,
        preamp_db: preamp,
        bands,
        graphic_gains_db: vec![0.0; 10],
    })
}

/// Ein Eintrag der öffentlichen AutoEQ-Datenbank.
#[derive(Debug, Clone)]
pub struct AutoEqEntry {
    /// Kopfhörer-Name, z. B. "Sennheiser HD 600".
    pub name: String,
    /// Quelle der Messung, z. B. "oratory1990".
    pub source: String,
    /// Pfad innerhalb der Datenbank (URL-kodiert).
    pub path: String,
}

const AUTOEQ_BASE: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results";

/// Zwischengespeicherte Liste aller Kopfhörer (ca. 850 kB Text). Wird nur
/// einmal pro App-Start geladen, nicht bei jedem Tastendruck in der Suche.
static AUTOEQ_INDEX: RwLock<Option<Vec<AutoEqEntry>>> = RwLock::new(None);

fn http() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?)
}

/// Zerlegt die Index-Datei. Jede Zeile sieht so aus:
/// `- [Sennheiser HD 600](./oratory1990/over-ear/Sennheiser%20HD%20600) by oratory1990`
fn parse_autoeq_index(text: &str) -> Vec<AutoEqEntry> {
    text.lines()
        .filter_map(|line| {
            let line = line.strip_prefix("- [")?;
            let (name, rest) = line.split_once("](./")?;
            let (path, rest) = rest.split_once(')')?;
            let source = rest.trim().strip_prefix("by ").unwrap_or("").to_owned();
            // Kopfhörernamen können selbst Klammern enthalten ("(ANC Off)"),
            // dann endet der Pfad erst an der LETZTEN Klammer vor " by".
            let (path, source) = match rest.rfind(") by ") {
                Some(i) => (format!("{path}){}", &rest[..i]), rest[i + 5..].trim().to_owned()),
                None => (path.to_owned(), source),
            };
            Some(AutoEqEntry { name: name.to_owned(), source, path })
        })
        .collect()
}

/// Sucht Kopfhörer in der AutoEQ-Datenbank (höchstens 50 Treffer).
pub fn autoeq_search(query: String) -> Result<Vec<AutoEqEntry>> {
    if AUTOEQ_INDEX.read().unwrap_or_else(|e| e.into_inner()).is_none() {
        let text = runtime::block_on_net(async {
            http()?
                .get(format!("{AUTOEQ_BASE}/INDEX.md"))
                .send()
                .await
                .context("AutoEQ-Datenbank nicht erreichbar")?
                .error_for_status()?
                .text()
                .await
                .map_err(anyhow::Error::from)
        })?;
        *AUTOEQ_INDEX.write().unwrap_or_else(|e| e.into_inner()) = Some(parse_autoeq_index(&text));
    }
    // Alle Suchwörter müssen vorkommen, Reihenfolge und Groß-/Kleinschreibung egal.
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let guard = AUTOEQ_INDEX.read().unwrap_or_else(|e| e.into_inner());
    let index = guard.as_ref().ok_or_else(|| anyhow!("AutoEQ-Liste fehlt"))?;
    Ok(index
        .iter()
        .filter(|e| {
            let name = e.name.to_lowercase();
            !words.is_empty() && words.iter().all(|w| name.contains(w))
        })
        .take(50)
        .cloned()
        .collect())
}

/// Lädt das Profil eines Kopfhörers und wandelt es in ein EQ-Profil um.
pub fn autoeq_fetch(entry: AutoEqEntry) -> Result<EqProfile> {
    // Die Datei heißt wie der Ordner: ".../<Name>/<Name> ParametricEQ.txt".
    let folder = entry.path.rsplit('/').next().unwrap_or(&entry.path);
    let url = format!("{AUTOEQ_BASE}/{}/{folder}%20ParametricEQ.txt", entry.path);
    let text = runtime::block_on_net(async {
        http()?
            .get(url)
            .send()
            .await
            .context("AutoEQ-Profil nicht erreichbar")?
            .error_for_status()?
            .text()
            .await
            .map_err(anyhow::Error::from)
    })?;
    parse_autoeq(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_autoeq_profile() {
        let text = "Preamp: -6.3 dB\n\
                    Filter 1: ON LSC Fc 105 Hz Gain 6.5 dB Q 0.70\n\
                    Filter 2: ON PK Fc 125 Hz Gain -2.7 dB Q 0.55\n\
                    Filter 3: OFF PK Fc 500 Hz Gain 1.0 dB Q 1.00\n\
                    Filter 4: ON HSC Fc 10000 Hz Gain -3.1 dB Q 0.70\n";
        let p = parse_autoeq(text.into()).unwrap();
        assert_eq!(p.preamp_db, -6.3);
        assert_eq!(p.bands.len(), 3); // der ausgeschaltete Filter fehlt
        assert_eq!(p.bands[0].kind, EqFilterKind::LowShelf);
        assert_eq!(p.bands[1].gain_db, -2.7);
        assert_eq!(p.bands[2].kind, EqFilterKind::HighShelf);
        assert!(parse_autoeq("kein Profil".into()).is_err());
    }

    #[test]
    fn parses_index_lines_including_parentheses() {
        let text = "# Index\n\
            - [Sennheiser HD 600](./oratory1990/over-ear/Sennheiser%20HD%20600) by oratory1990\n\
            - [1MORE Aero (ANC Off)](./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045\n";
        let e = parse_autoeq_index(text);
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].name, "Sennheiser HD 600");
        assert_eq!(e[0].path, "oratory1990/over-ear/Sennheiser%20HD%20600");
        assert_eq!(e[0].source, "oratory1990");
        assert_eq!(e[1].path, "HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)");
        assert_eq!(e[1].source, "HypetheSonics on GRAS RA0045");
    }

    #[test]
    fn flat_profile_has_flat_curve() {
        let curve = eq_response(EqProfile::default(), 32);
        assert_eq!(curve.len(), 32);
        assert!((curve[0].freq_hz - 20.0).abs() < 0.1);
        assert!((curve[31].freq_hz - 20_000.0).abs() < 5.0);
        assert!(curve.iter().all(|p| p.gain_db.abs() < 0.01));
    }
}
