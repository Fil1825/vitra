//! The music's spectrum for the level meter module: how loud each of twenty bands from
//! 40 Hz to 16 kHz is, read from mpv's audio chain like the backdrop's pulse (`level.rs`),
//! so it works for files and streams alike.
//!
//! mpv only lets us read the metadata of the frames a filter *outputs*, and every lavfi
//! filter that merges two branches (`amerge`, `amix`, `join`) drops the metadata of the
//! side branch. So the analysis rides along in the audio itself: the stereo frames get one
//! extra channel per band (a mono mix of the music), each band's channel goes through its
//! own band-pass filter (`bandpass … c=<channel>` touches only that channel), `astats`
//! measures every channel's RMS level, and a last `pan` keeps only the first two channels,
//! which are the music bit for bit. The graph runs on one thread: lavfi's own threading
//! costs ten times more than the filtering.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use serde_json::Value;

pub const SPECTRUM_EVENT: &str = "player://spectrum";

/// The filter's label in mpv's audio chain (`af-metadata/<label>`).
pub const FILTER_LABEL: &str = "vitraspectrum";

/// How many bands, and the centres of the lowest and highest.
pub const BANDS: usize = 20;
const LOWEST_HZ: f64 = 40.0;
const HIGHEST_HZ: f64 = 16_000.0;

/// Channel names for the bands after the music's FL and FR: `bandpass` picks its channel by
/// name, so each band needs one of FFmpeg's named channels.
const CHANNELS: [&str; BANDS] = [
    "FC", "LFE", "BL", "BR", "FLC", "FRC", "BC", "SL", "SR", "TC", "TFL", "TFC", "TFR", "TBL",
    "TBC", "TBR", "DL", "DR", "WL", "WR",
];

/// How often the spectrum is read while playing. mpv fills its audio buffer in bursts, so
/// readings change a few times a second; the window eases between them.
pub const POLL: Duration = Duration::from_millis(33);

/// mpv filters the audio this much before it is heard (its buffer, 0.2 s by default, plus
/// what it decodes ahead): readings wait as long, so the bars move with what plays.
pub const DELAY: Duration = Duration::from_millis(330);

/// The bands' centre frequencies, evenly spaced on a logarithmic scale.
pub fn centers() -> [f64; BANDS] {
    let step = (HIGHEST_HZ / LOWEST_HZ).powf(1.0 / (BANDS - 1) as f64);
    std::array::from_fn(|band| LOWEST_HZ * step.powi(band as i32))
}

/// The `af add` argument.
pub fn filter() -> String {
    let step = (HIGHEST_HZ / LOWEST_HZ).powf(1.0 / (BANDS - 1) as f64);
    // Each band reaches its neighbours' centres, so together they cover the range.
    let octaves = step.log2();
    let layout = ["FL", "FR"]
        .into_iter()
        .chain(CHANNELS)
        .collect::<Vec<_>>()
        .join("+");
    let mixes = (0..BANDS)
        .map(|band| format!("c{}=0.5*c0+0.5*c1", band + 2))
        .collect::<Vec<_>>()
        .join("|");
    let bands = centers()
        .iter()
        .zip(CHANNELS)
        .map(|(hz, channel)| format!("bandpass=f={hz:.0}:width_type=o:w={octaves:.3}:c={channel}"))
        .collect::<Vec<_>>()
        .join(",");
    // Mono and surround music becomes stereo here, as most outputs play it anyway.
    format!(
        "@{FILTER_LABEL}:lavfi=graph=[aformat=channel_layouts=stereo,\
         pan={layout}|c0=c0|c1=c1|{mixes},{bands},\
         astats=metadata=1:reset=2:measure_perchannel=RMS_level:measure_overall=none,\
         pan=stereo|c0=c0|c1=c1]:o=[threads=1]"
    )
}

/// Each band's RMS level (dBFS) in the filter's metadata; `None` without a reading. A band
/// with nothing in it ("-inf") is very quiet.
pub fn band_levels(metadata: &Value) -> Option<[f64; BANDS]> {
    let mut levels = [f64::NEG_INFINITY; BANDS];
    let mut found = false;
    for (band, level) in levels.iter_mut().enumerate() {
        // astats numbers channels from 1, and the bands come after FL and FR.
        let key = format!("lavfi.astats.{}.RMS_level", band + 3);
        if let Some(text) = metadata.get(&key).and_then(Value::as_str) {
            found = true;
            *level = text.parse::<f64>().unwrap_or(f64::NEG_INFINITY);
        }
    }
    found.then_some(levels)
}

/// A band's level as the meter shows it, 0..1. Music has less energy up high, so higher
/// bands are lifted a little (1.5 dB per octave around 1 kHz) to look balanced; the range
/// covers quiet passages to loud masters.
pub fn normalize(band: usize, db: f64) -> f32 {
    const FLOOR: f64 = -60.0;
    const CEILING: f64 = -12.0;
    if !db.is_finite() {
        return 0.0;
    }
    let tilt = 1.5 * (centers()[band.min(BANDS - 1)] / 1000.0).log2();
    let value = ((db + tilt - FLOOR) / (CEILING - FLOOR)).clamp(0.0, 1.0);
    // Two decimals are plenty for bars, and keep the events small.
    ((value * 100.0).round() / 100.0) as f32
}

/// The whole reading, ready to send.
pub fn reading(metadata: &Value) -> Option<Vec<f32>> {
    let levels = band_levels(metadata)?;
    Some(
        levels
            .iter()
            .enumerate()
            .map(|(band, db)| normalize(band, *db))
            .collect(),
    )
}

/// Holds readings back by [`DELAY`] so they show when their audio plays.
#[derive(Debug, Default)]
pub struct DelayLine {
    queue: VecDeque<(Instant, Vec<f32>)>,
}

impl DelayLine {
    /// A reading taken at `at`; the same values as the last one aren't queued again.
    pub fn push(&mut self, at: Instant, values: Vec<f32>) {
        if self.queue.back().is_some_and(|(_, last)| *last == values) {
            return;
        }
        self.queue.push_back((at, values));
    }

    /// The newest reading that is due at `now`, if one became due since the last call.
    pub fn take(&mut self, now: Instant) -> Option<Vec<f32>> {
        let mut due = None;
        while self
            .queue
            .front()
            .is_some_and(|(at, _)| now.duration_since(*at) >= DELAY)
        {
            due = self.queue.pop_front().map(|(_, values)| values);
        }
        due
    }

    /// Forgets what is waiting (a seek, a pause, another track).
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bands_span_the_range_on_a_log_scale() {
        let centers = centers();
        assert!((centers[0] - 40.0).abs() < 1e-9);
        assert!((centers[BANDS - 1] - 16_000.0).abs() < 1e-6);
        let ratio = centers[1] / centers[0];
        for pair in centers.windows(2) {
            assert!((pair[1] / pair[0] - ratio).abs() < 1e-9);
        }
    }

    #[test]
    fn the_filter_names_each_band_once_and_keeps_the_music() {
        let filter = filter();
        assert!(filter.starts_with("@vitraspectrum:lavfi=graph=["));
        assert!(filter.ends_with(",pan=stereo|c0=c0|c1=c1]:o=[threads=1]"));
        assert_eq!(filter.matches("bandpass=").count(), BANDS);
        // Every band-pass filter is followed by another filter.
        for channel in CHANNELS {
            assert_eq!(
                filter.matches(&format!(":c={channel},")).count(),
                1,
                "{channel}"
            );
        }
        // Every channel name is distinct, so no band is measured twice.
        let mut names = CHANNELS.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BANDS);
    }

    #[test]
    fn reads_the_bands_after_the_music_channels() {
        let mut metadata = serde_json::Map::new();
        metadata.insert("lavfi.astats.1.RMS_level".into(), json!("-3.0"));
        metadata.insert("lavfi.astats.2.RMS_level".into(), json!("-3.0"));
        for channel in 3..=BANDS + 2 {
            metadata.insert(
                format!("lavfi.astats.{channel}.RMS_level"),
                json!(format!("-{}", 10 + channel)),
            );
        }
        metadata.insert("lavfi.astats.5.RMS_level".into(), json!("-inf"));
        let levels = band_levels(&Value::Object(metadata)).unwrap();
        assert_eq!(levels[0], -13.0);
        assert_eq!(levels[2], f64::NEG_INFINITY);
        assert_eq!(levels[BANDS - 1], -(10.0 + (BANDS + 2) as f64));
        assert_eq!(band_levels(&json!({})), None);
        assert_eq!(band_levels(&Value::Null), None);
    }

    #[test]
    fn levels_map_into_the_meter_range() {
        assert_eq!(normalize(0, f64::NEG_INFINITY), 0.0);
        assert_eq!(normalize(5, -120.0), 0.0);
        assert_eq!(normalize(5, 0.0), 1.0);
        // The same level looks taller up high than down low.
        assert!(normalize(BANDS - 1, -40.0) > normalize(0, -40.0));
        for band in 0..BANDS {
            let value = normalize(band, -30.0);
            assert!((0.0..=1.0).contains(&value), "{band}: {value}");
        }
    }

    #[test]
    fn readings_wait_for_their_audio() {
        let start = Instant::now();
        let mut line = DelayLine::default();
        line.push(start, vec![0.1]);
        line.push(start + Duration::from_millis(10), vec![0.1]);
        line.push(start + Duration::from_millis(100), vec![0.5]);
        assert_eq!(line.take(start + Duration::from_millis(100)), None);
        assert_eq!(line.take(start + DELAY), Some(vec![0.1]));
        assert_eq!(line.take(start + DELAY), None);
        // Late callers get the newest reading that is due, not a backlog.
        line.push(start + Duration::from_millis(120), vec![0.7]);
        assert_eq!(
            line.take(start + DELAY + Duration::from_millis(200)),
            Some(vec![0.7])
        );
        line.push(start, vec![0.9]);
        line.clear();
        assert_eq!(line.take(start + Duration::from_secs(9)), None);
    }
}
