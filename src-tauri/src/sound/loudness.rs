//! Evening out loudness (pure parts): the gain a measured track gets, reading FFmpeg's
//! EBU R128 summary, and integrating mpv's momentary readings for links.
//!
//! mpv applies the gain: ReplayGain tags where a file has them (`replaygain=track|album`,
//! with a preamp from ReplayGain's reference to Vitra's target), and for other tracks the
//! gain from Vitra's own measurement as that file's `replaygain-fallback`, which mpv uses
//! exactly when the file has no tags.

use library::Loudness;

/// How loud every track plays: about where streaming services put music.
pub const TARGET_LUFS: f64 = -14.0;
/// ReplayGain 2's reference loudness; tags are gains towards it.
const REPLAYGAIN_REFERENCE_LUFS: f64 = -18.0;
/// The preamp that moves ReplayGain's reference to [`TARGET_LUFS`].
pub const REPLAYGAIN_PREAMP: f64 = TARGET_LUFS - REPLAYGAIN_REFERENCE_LUFS;
/// Quiet tracks are raised only as far as their peaks stay below this (dBTP).
const PEAK_CEILING: f64 = -1.0;
const MOST_CUT: f64 = -20.0;
const MOST_BOOST: f64 = 12.0;
/// A track whose peak wasn't measured (a link) is raised this much at most.
const BLIND_BOOST: f64 = 3.0;

/// The gain (dB) that brings a measured track to [`TARGET_LUFS`] without clipping.
pub fn gain(loudness: Loudness) -> f64 {
    let mut gain = TARGET_LUFS - loudness.integrated;
    gain = gain.min(
        loudness
            .peak
            .map_or(BLIND_BOOST, |peak| PEAK_CEILING - peak),
    );
    if !gain.is_finite() {
        return 0.0;
    }
    (gain.clamp(MOST_CUT, MOST_BOOST) * 10.0).round() / 10.0
}

/// The integrated loudness and true peak from the summary FFmpeg's `ebur128` filter prints
/// at the end (`I: -9.3 LUFS`, `Peak: 0.4 dBFS`).
pub fn parse_ebur128(output: &str) -> Option<Loudness> {
    let summary = &output[output.rfind("Summary:")?..];
    let value = |label: &str| {
        summary.lines().find_map(|line| {
            let rest = line.trim().strip_prefix(label)?;
            rest.split_whitespace().next()?.parse::<f64>().ok()
        })
    };
    let integrated = value("I:").filter(|lufs| lufs.is_finite() && *lufs > -70.0)?;
    Some(Loudness {
        integrated,
        peak: value("Peak:").filter(|db| db.is_finite()),
    })
}

/// EBU R128's integrated loudness from momentary readings (400 ms windows) taken while a
/// track plays: blocks below −70 LUFS don't count, nor those more than 10 LU below the
/// average of the rest. mpv only hands out the filter's latest values, so a link is measured
/// from readings sampled a few times a second, which comes within a fraction of a decibel.
#[derive(Debug, Default)]
pub struct Integrator {
    /// Each reading's energy.
    blocks: Vec<f64>,
}

/// How often mpv's readings are taken while a link plays.
pub const READING_INTERVAL: std::time::Duration = std::time::Duration::from_millis(330);
/// Readings needed before a measurement counts: about half a minute.
const MIN_BLOCKS: usize = 90;
/// …and at least this share of the track.
const MIN_SHARE: f64 = 0.4;

fn energy(lufs: f64) -> f64 {
    10f64.powf((lufs + 0.691) / 10.0)
}

fn lufs(energy: f64) -> f64 {
    -0.691 + 10.0 * energy.log10()
}

impl Integrator {
    pub fn push(&mut self, momentary: f64) {
        if momentary.is_finite() && momentary > -70.0 {
            self.blocks.push(energy(momentary));
        }
    }

    #[cfg(test)]
    fn clear(&mut self) {
        self.blocks.clear();
    }

    /// The integrated loudness so far, once enough of a track of `duration` seconds (when
    /// known) was heard for it to stand for the whole.
    pub fn integrated(&self, duration: Option<f64>) -> Option<f64> {
        let heard = self.blocks.len() as f64 * READING_INTERVAL.as_secs_f64();
        let enough = duration.is_none_or(|duration| heard >= duration * MIN_SHARE);
        if self.blocks.len() < MIN_BLOCKS || !enough {
            return None;
        }
        let mean = |blocks: &mut dyn Iterator<Item = f64>| {
            let (sum, count) = blocks.fold((0.0, 0usize), |(s, n), e| (s + e, n + 1));
            (count > 0).then(|| sum / count as f64)
        };
        let ungated = mean(&mut self.blocks.iter().copied())?;
        let gate = energy(lufs(ungated) - 10.0);
        mean(&mut self.blocks.iter().copied().filter(|e| *e >= gate)).map(lufs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loud_tracks_go_down_and_quiet_ones_up_as_far_as_their_peaks_allow() {
        let loud = Loudness {
            integrated: -7.0,
            peak: Some(1.2),
        };
        assert_eq!(gain(loud), -7.0);
        let quiet = Loudness {
            integrated: -24.0,
            peak: Some(-6.0),
        };
        // 10 dB would be needed, the peak allows 5.
        assert_eq!(gain(quiet), 5.0);
        // Without a peak, a quiet link is raised only a little.
        let unknown_peak = Loudness {
            integrated: -20.0,
            peak: None,
        };
        assert_eq!(gain(unknown_peak), BLIND_BOOST);
        let silent = Loudness {
            integrated: -60.0,
            peak: Some(-50.0),
        };
        assert_eq!(gain(silent), MOST_BOOST);
        assert!((REPLAYGAIN_PREAMP - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn reads_ffmpeg_summaries() {
        let output = "\
[Parsed_ebur128_0 @ 0x1] t: 149.9  TARGET:-23 LUFS    M: -21.8 S: -21.8     I: -21.8 LUFS
[Parsed_ebur128_0 @ 0x5630fa024700] Summary:

  Integrated loudness:
    I:         -21.8 LUFS
    Threshold: -31.8 LUFS

  Loudness range:
    LRA:         0.0 LU

  True peak:
    Peak:      -18.1 dBFS
";
        assert_eq!(
            parse_ebur128(output),
            Some(Loudness {
                integrated: -21.8,
                peak: Some(-18.1),
            })
        );
        // Silence measures as -70 or -inf: nothing to normalize.
        assert_eq!(parse_ebur128("Summary:\n    I:         -70.0 LUFS\n"), None);
        assert_eq!(parse_ebur128("no summary here"), None);
    }

    #[test]
    fn integrates_momentary_readings_with_the_gates() {
        let mut integrator = Integrator::default();
        for _ in 0..80 {
            integrator.push(-10.0);
        }
        assert_eq!(integrator.integrated(None), None, "too short");
        for _ in 0..80 {
            integrator.push(-10.0);
        }
        // A quiet passage below the relative gate and silence don't pull it down.
        for _ in 0..40 {
            integrator.push(-30.0);
            integrator.push(f64::NEG_INFINITY);
        }
        let integrated = integrator.integrated(Some(120.0)).unwrap();
        assert!((integrated + 10.0).abs() < 0.01, "{integrated}");
        // Not enough of a long track.
        assert_eq!(integrator.integrated(Some(600.0)), None);
        integrator.clear();
        assert_eq!(integrator.integrated(None), None);
    }
}
