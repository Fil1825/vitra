//! Sound settings: the equalizer, volume normalization and crossfade, and the equalizer's
//! filter in mpv's audio chain.
//!
//! The equalizer is ten peaking filters an octave wide at the usual centres, then a preamp,
//! in one lavfi graph. Its instances are named (`equalizer@b3`, `volume@pre`), so a changed
//! gain is sent to that one filter (`af-command`) while the music plays, without rebuilding
//! the chain (which could click). A flat or switched-off equalizer is taken out of the chain.

pub mod loudness;
pub mod measure;

use std::ops::RangeInclusive;

use serde::{Deserialize, Serialize};

use crate::settings::or_default;

pub use measure::LoudnessMeter;

/// The bands' centres (Hz).
pub const BANDS: [f64; 10] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
];
/// A band's gain and the preamp, in dB.
pub const GAINS: RangeInclusive<f64> = -12.0..=12.0;
/// The longest crossfade, in seconds (0 plays gapless).
pub const CROSSFADE: RangeInclusive<f64> = 0.0..=12.0;
/// Presets of one's own, at most.
const MAX_PRESETS: usize = 30;
const MAX_PRESET_NAME: usize = 60;

/// The equalizer's label in mpv's audio chain.
pub const EQ_LABEL: &str = "vitraeq";
/// The loudness meter's label: EBU R128 readings for links, which can't be measured ahead.
pub const LOUDNESS_LABEL: &str = "vitraloud";

/// The loudness meter in mpv's chain, while normalization is on.
pub fn loudness_filter() -> String {
    format!("@{LOUDNESS_LABEL}:lavfi=graph=[ebur128=metadata=1]:o=[threads=1]")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SoundSettings {
    #[serde(deserialize_with = "or_default")]
    pub equalizer: Equalizer,
    /// Equalizer presets the user saved.
    #[serde(deserialize_with = "or_default")]
    pub presets: Vec<EqPreset>,
    #[serde(deserialize_with = "or_default")]
    pub normalization: Normalization,
    /// Seconds two tracks overlap when one follows another; 0 plays them gapless.
    #[serde(deserialize_with = "or_default")]
    pub crossfade: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Equalizer {
    #[serde(deserialize_with = "or_default")]
    pub enabled: bool,
    /// Which preset the gains came from, for the picker (`flat`, `rock`, `user:<name>`…);
    /// empty once the user moved a band.
    #[serde(deserialize_with = "or_default")]
    pub preset: String,
    /// dB per band, low to high.
    #[serde(deserialize_with = "or_default")]
    pub gains: Vec<f64>,
    /// dB before the bands, to keep boosts from clipping.
    #[serde(deserialize_with = "or_default")]
    pub preamp: f64,
}

impl Default for Equalizer {
    fn default() -> Self {
        Self {
            enabled: false,
            preset: "flat".into(),
            gains: vec![0.0; BANDS.len()],
            preamp: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EqPreset {
    pub name: String,
    pub gains: Vec<f64>,
    pub preamp: f64,
}

/// Evening out loudness between tracks: ReplayGain tags where files have them, Vitra's own
/// measurement elsewhere.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Normalization {
    #[default]
    Off,
    /// Every track at the same loudness.
    Track,
    /// Albums keep their own dynamics (the album gain where tags have one).
    Album,
}

/// A gain within [`GAINS`], in tenths of a dB (finer steps aren't heard).
fn gain(value: f64) -> f64 {
    if value.is_finite() {
        (value.clamp(*GAINS.start(), *GAINS.end()) * 10.0).round() / 10.0
    } else {
        0.0
    }
}

fn gains(values: &[f64]) -> Vec<f64> {
    (0..BANDS.len())
        .map(|band| gain(values.get(band).copied().unwrap_or(0.0)))
        .collect()
}

impl SoundSettings {
    /// Normalizes what serde cannot check.
    pub fn sanitized(mut self) -> Self {
        self.equalizer = self.equalizer.sanitized();
        let mut presets: Vec<EqPreset> = Vec::new();
        for preset in std::mem::take(&mut self.presets) {
            let name: String = preset.name.trim().chars().take(MAX_PRESET_NAME).collect();
            if name.is_empty() || presets.iter().any(|known| known.name == name) {
                continue;
            }
            presets.push(EqPreset {
                name,
                gains: gains(&preset.gains),
                preamp: gain(preset.preamp),
            });
        }
        presets.truncate(MAX_PRESETS);
        self.presets = presets;
        self.crossfade = if self.crossfade.is_finite() {
            (self.crossfade.clamp(*CROSSFADE.start(), *CROSSFADE.end()) * 2.0).round() / 2.0
        } else {
            0.0
        };
        self
    }
}

impl Equalizer {
    pub fn sanitized(mut self) -> Self {
        self.gains = gains(&self.gains);
        self.preamp = gain(self.preamp);
        self.preset = self.preset.chars().take(MAX_PRESET_NAME + 5).collect();
        self
    }

    /// Whether the equalizer changes the sound at all; a flat one isn't worth its filters.
    pub fn active(&self) -> bool {
        self.enabled && (self.preamp != 0.0 || self.gains.iter().any(|gain| *gain != 0.0))
    }

    fn band_gain(&self, band: usize) -> f64 {
        self.gains.get(band).copied().unwrap_or(0.0)
    }

    /// The `af add` argument.
    pub fn filter(&self) -> String {
        let bands = BANDS
            .iter()
            .enumerate()
            .map(|(band, hz)| {
                format!(
                    "equalizer@b{band}=f={hz}:t=o:w=1:g={:.1}",
                    self.band_gain(band)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "@{EQ_LABEL}:lavfi=graph=[{bands},volume@pre=volume={:.1}dB]:o=[threads=1]",
            self.preamp
        )
    }

    /// The `af-command` arguments (command, value, filter instance) that turn the filter of
    /// `from` into this one.
    pub fn commands(&self, from: &Self) -> Vec<[String; 3]> {
        let mut commands: Vec<[String; 3]> = (0..BANDS.len())
            .filter(|band| self.band_gain(*band) != from.band_gain(*band))
            .map(|band| {
                [
                    "g".into(),
                    format!("{:.1}", self.band_gain(band)),
                    format!("equalizer@b{band}"),
                ]
            })
            .collect();
        if self.preamp != from.preamp {
            commands.push([
                "volume".into(),
                format!("{:.1}dB", self.preamp),
                "volume@pre".into(),
            ]);
        }
        commands
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eq(gains: &[f64], preamp: f64) -> Equalizer {
        Equalizer {
            enabled: true,
            preset: String::new(),
            gains: gains.to_vec(),
            preamp,
        }
    }

    #[test]
    fn values_are_kept_in_range() {
        let settings = SoundSettings {
            equalizer: eq(&[20.0, -30.0, f64::NAN, 1.234], 99.0),
            presets: vec![
                EqPreset {
                    name: "  Mine ".into(),
                    gains: vec![3.0; 12],
                    preamp: -2.0,
                },
                EqPreset {
                    name: "Mine".into(),
                    gains: vec![],
                    preamp: 0.0,
                },
                EqPreset {
                    name: " ".into(),
                    gains: vec![],
                    preamp: 0.0,
                },
            ],
            normalization: Normalization::Album,
            crossfade: 30.0,
        }
        .sanitized();
        assert_eq!(
            settings.equalizer.gains,
            vec![12.0, -12.0, 0.0, 1.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(settings.equalizer.preamp, 12.0);
        assert_eq!(settings.presets.len(), 1);
        assert_eq!(settings.presets[0].name, "Mine");
        assert_eq!(settings.presets[0].gains.len(), BANDS.len());
        assert_eq!(settings.crossfade, 12.0);
        assert_eq!(
            SoundSettings {
                crossfade: 2.3,
                ..SoundSettings::default()
            }
            .sanitized()
            .crossfade,
            2.5
        );
    }

    #[test]
    fn a_flat_or_switched_off_equalizer_is_left_out() {
        assert!(!Equalizer::default().active());
        assert!(!eq(&[0.0; 10], 0.0).active());
        assert!(eq(&[0.0; 10], -3.0).active());
        let mut boosted = eq(&[0.0, 4.0], 0.0);
        assert!(boosted.active());
        boosted.enabled = false;
        assert!(!boosted.active());
    }

    #[test]
    fn the_filter_names_every_band() {
        let filter = eq(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, -1.5], -4.0).filter();
        assert!(filter.starts_with("@vitraeq:lavfi=graph=[equalizer@b0=f=32:t=o:w=1:g=1.0,"));
        assert!(filter.contains("equalizer@b9=f=16000:t=o:w=1:g=-1.5,volume@pre=volume=-4.0dB]"));
        assert!(filter.ends_with(":o=[threads=1]"));
        assert_eq!(filter.matches("equalizer@").count(), BANDS.len());
    }

    #[test]
    fn only_changed_bands_are_sent() {
        let from = eq(&[0.0, 2.0, 0.0], 0.0);
        let to = eq(&[0.0, 2.0, 5.5], -2.0);
        assert_eq!(
            to.commands(&from),
            vec![
                ["g".to_owned(), "5.5".into(), "equalizer@b2".into()],
                ["volume".to_owned(), "-2.0dB".into(), "volume@pre".into()],
            ]
        );
        assert!(to.commands(&to).is_empty());
    }
}
