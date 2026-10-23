//! A gentle loudness signal for the animated backdrop. mpv's `astats` filter measures the
//! audio (before the volume control, so the pulse doesn't depend on it); while a window
//! wants it, the player reads the level a dozen times a second and smooths it into 0..1.

use std::time::Duration;

use serde_json::Value;

pub const LEVEL_EVENT: &str = "player://level";

/// The filter's label in mpv's audio chain (`af-metadata/<label>`).
pub const FILTER_LABEL: &str = "vitralevel";

/// How often the level is read while playing; the windows ease between readings.
pub const POLL: Duration = Duration::from_millis(70);

/// How often to look again while nothing plays.
pub const IDLE_POLL: Duration = Duration::from_millis(300);

/// The `af add` argument. Only the overall RMS level is measured, over two audio frames
/// (some 50 ms), which costs next to nothing.
pub fn filter() -> String {
    format!(
        "@{FILTER_LABEL}:lavfi=[astats=metadata=1:reset=2:measure_perchannel=none:\
         measure_overall=RMS_level]"
    )
}

/// The RMS level (dBFS) in the filter's metadata; `None` for silence ("-inf") or nothing.
pub fn rms_db(metadata: &Value) -> Option<f64> {
    metadata
        .get("lavfi.astats.Overall.RMS_level")?
        .as_str()?
        .parse::<f64>()
        .ok()
        .filter(|db| db.is_finite())
}

/// Turns readings into the pulse: a quick rise and a slower fall, so it breathes rather than
/// flickers, plus a kick when the sound gets louder than it has been lately (a beat).
#[derive(Debug, Default)]
pub struct Envelope {
    fast: f64,
    slow: f64,
}

impl Envelope {
    /// The next pulse, 0..1, from a reading (`None`: silence or nothing playing).
    pub fn next(&mut self, rms_db: Option<f64>) -> f64 {
        // Music sits around -30 to -6 dBFS.
        let loudness = rms_db.map_or(0.0, |db| ((db + 36.0) / 30.0).clamp(0.0, 1.0));
        let rate = if loudness > self.fast { 0.6 } else { 0.18 };
        self.fast += (loudness - self.fast) * rate;
        self.slow += (loudness - self.slow) * 0.04;
        let kick = ((self.fast - self.slow) * 3.0).max(0.0);
        (self.fast * 0.35 + kick).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_level_mpv_reports() {
        let metadata = json!({"lavfi.astats.Overall.RMS_level": "-21.074874"});
        assert_eq!(rms_db(&metadata), Some(-21.074874));
        assert_eq!(
            rms_db(&json!({"lavfi.astats.Overall.RMS_level": "-inf"})),
            None
        );
        assert_eq!(rms_db(&json!({})), None);
        assert!(filter().starts_with("@vitralevel:lavfi=[astats="));
    }

    #[test]
    fn beats_kick_and_silence_settles() {
        let mut envelope = Envelope::default();
        // A steady quiet passage settles low.
        let mut quiet = 0.0;
        for _ in 0..200 {
            quiet = envelope.next(Some(-30.0));
        }
        assert!(quiet < 0.15, "{quiet}");
        // A beat kicks well above it at once.
        let beat = envelope.next(Some(-8.0));
        assert!(beat > quiet + 0.4, "{beat}");
        // Silence brings it back down within about a second.
        let mut after = beat;
        for _ in 0..14 {
            after = envelope.next(None);
        }
        assert!(after < 0.05, "{after}");
        for _ in 0..100 {
            let pulse = envelope.next(Some(0.0));
            assert!((0.0..=1.0).contains(&pulse));
        }
    }
}
