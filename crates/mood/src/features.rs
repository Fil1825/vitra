//! What a track sounds like, from up to a minute and a half of it decoded to mono PCM at
//! 22 050 Hz: how loud and how steady, how fast, how bright, major or minor. FFmpeg only
//! decodes; everything is measured here, so the same code serves files and streams and is
//! tested on generated signals.

use serde::{Deserialize, Serialize};

use crate::fft::{Fft, hann};

/// The rate FFmpeg decodes to for the analysis.
pub const SAMPLE_RATE: u32 = 22_050;
/// Bumped whenever the analysis changes: every track is analyzed again once.
pub const VERSION: i64 = 1;

/// The tempo range a beat is folded into (half and double time are the same feel).
pub const TEMPO_MIN: f32 = 70.0;
pub const TEMPO_MAX: f32 = 180.0;

const FRAME: usize = 2048;
const HOP: usize = 512;
/// Long frames for pitch: 2.7 Hz bins tell semitones apart down to about 80 Hz.
const CHROMA_FRAME: usize = 8192;
/// Below this a block or frame is silence (gaps, fades) and doesn't count.
const SILENCE_DB: f32 = -60.0;
/// Loudness is followed in half-second blocks.
const BLOCK_SECONDS: f32 = 0.5;
/// Less audible sound than this tells nothing.
const MIN_SECONDS: f32 = 5.0;
/// Mean onset strength from which a beat counts in full (soft music reaches it too).
const ONSET_REFERENCE: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// Mean loudness of the audible part, dBFS (about −40 for quiet music, −8 for loud).
    pub energy: f32,
    /// How much the loudness moves: the spread of half-second loudness, dB.
    pub spread: f32,
    /// Beats per minute, folded into 70–180.
    pub tempo: f32,
    /// How clearly there is a beat at all, 0–1.
    pub tempo_confidence: f32,
    /// The mean spectral centroid, Hz: dark (low) to bright (high).
    pub brightness: f32,
    /// Major (towards 1) or minor (towards −1), from how the notes fit the two key profiles.
    pub valence: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisError {
    /// Fewer than five seconds of audio.
    TooShort,
    /// Next to nothing audible.
    Silent,
}

impl std::fmt::Display for AnalysisError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::TooShort => "too short to analyze",
            Self::Silent => "silent",
        })
    }
}

impl std::error::Error for AnalysisError {}

/// Measures `samples` (mono, [`SAMPLE_RATE`]).
pub fn analyze(samples: &[f32]) -> Result<Features, AnalysisError> {
    let rate = SAMPLE_RATE as f32;
    if (samples.len() as f32) < MIN_SECONDS * rate {
        return Err(AnalysisError::TooShort);
    }
    let (energy, spread, audible) = loudness(samples);
    if audible < MIN_SECONDS {
        return Err(AnalysisError::Silent);
    }
    let (brightness, onsets) = spectrum(samples);
    let (tempo, tempo_confidence) = tempo(&onsets, rate / HOP as f32);
    let valence = valence(&chroma(samples));
    Ok(Features {
        energy,
        spread,
        tempo,
        tempo_confidence,
        brightness,
        valence,
    })
}

fn db(power: f32) -> f32 {
    10.0 * power.max(1e-12).log10()
}

/// Mean loudness (energy-averaged over the audible blocks), its spread (dB) and how many
/// seconds were audible.
fn loudness(samples: &[f32]) -> (f32, f32, f32) {
    let block = (BLOCK_SECONDS * SAMPLE_RATE as f32) as usize;
    let powers: Vec<f32> = samples
        .chunks(block)
        .filter(|chunk| chunk.len() == block)
        .map(|chunk| chunk.iter().map(|s| s * s).sum::<f32>() / block as f32)
        .filter(|power| db(*power) > SILENCE_DB)
        .collect();
    if powers.is_empty() {
        return (SILENCE_DB, 0.0, 0.0);
    }
    let count = powers.len() as f32;
    let energy = db(powers.iter().sum::<f32>() / count);
    let levels: Vec<f32> = powers.iter().map(|p| db(*p)).collect();
    let mean = levels.iter().sum::<f32>() / count;
    let spread = (levels.iter().map(|l| (l - mean).powi(2)).sum::<f32>() / count).sqrt();
    (energy, spread, count * BLOCK_SECONDS)
}

/// The mean spectral centroid of the audible frames (Hz) and the onset strength of every
/// frame: how much louder each frequency got since the frame before (log-compressed
/// spectral flux), whose rhythm is the beat.
fn spectrum(samples: &[f32]) -> (f32, Vec<f32>) {
    let fft = Fft::new(FRAME);
    let window = hann(FRAME);
    let bin_hz = SAMPLE_RATE as f32 / FRAME as f32;
    let mut frame = vec![0.0; FRAME];
    let mut magnitudes = Vec::new();
    let mut previous: Vec<f32> = Vec::new();
    let mut onsets = Vec::new();
    let mut centroids = 0.0;
    let mut counted = 0usize;
    let mut start = 0;
    while start + FRAME <= samples.len() {
        let slice = &samples[start..start + FRAME];
        let power = slice.iter().map(|s| s * s).sum::<f32>() / FRAME as f32;
        for ((out, sample), w) in frame.iter_mut().zip(slice).zip(&window) {
            *out = sample * w;
        }
        fft.magnitudes(&frame, &mut magnitudes);
        let compressed: Vec<f32> = magnitudes.iter().map(|m| m.ln_1p()).collect();
        let flux = if previous.is_empty() {
            0.0
        } else {
            compressed
                .iter()
                .zip(&previous)
                .map(|(now, before)| (now - before).max(0.0))
                .sum()
        };
        onsets.push(flux);
        previous = compressed;
        if db(power) > SILENCE_DB {
            let total: f32 = magnitudes.iter().sum();
            if total > 0.0 {
                let weighted: f32 = magnitudes
                    .iter()
                    .enumerate()
                    .map(|(k, m)| k as f32 * bin_hz * m)
                    .sum();
                centroids += weighted / total;
                counted += 1;
            }
        }
        start += HOP;
    }
    let brightness = if counted > 0 {
        centroids / counted as f32
    } else {
        0.0
    };
    (brightness, onsets)
}

/// The tempo (BPM, folded into 70–180) and how sure it is (0–1): the strongest
/// periodicity of the onset strength, leaning towards 120 BPM the way listeners do (a
/// log-normal prior one octave wide), refined between frames.
fn tempo(onsets: &[f32], fps: f32) -> (f32, f32) {
    let count = onsets.len();
    if count < 16 {
        return (120.0, 0.0);
    }
    let mean = onsets.iter().sum::<f32>() / count as f32;
    let centred: Vec<f32> = onsets.iter().map(|o| o - mean).collect();
    let zero: f32 = centred.iter().map(|x| x * x).sum();
    if zero <= f32::EPSILON {
        return (120.0, 0.0);
    }
    let bpm_at = |lag: f32| 60.0 * fps / lag;
    let shortest = (60.0 * fps / 220.0).floor().max(1.0) as usize;
    let longest = ((60.0 * fps / 40.0).ceil() as usize).min(count - 2);
    if shortest + 2 > longest {
        return (120.0, 0.0);
    }
    // Normalized and unbiased: a long lag sums fewer products.
    let correlation = |lag: usize| -> f32 {
        let sum: f32 = centred[..count - lag]
            .iter()
            .zip(&centred[lag..])
            .map(|(a, b)| a * b)
            .sum();
        sum / zero * count as f32 / (count - lag) as f32
    };
    let values: Vec<f32> = (0..=longest + 1).map(correlation).collect();
    let prior = |bpm: f32| (-0.5 * (bpm / 120.0).log2().powi(2)).exp();
    let best = (shortest..=longest)
        .max_by(|&a, &b| {
            (values[a] * prior(bpm_at(a as f32))).total_cmp(&(values[b] * prior(bpm_at(b as f32))))
        })
        .unwrap_or(shortest);
    // A parabola through the peak and its neighbours puts it between frames.
    let (left, peak, right) = (values[best - 1], values[best], values[best + 1]);
    let curve = left - 2.0 * peak + right;
    let offset = if curve < 0.0 {
        (0.5 * (left - right) / curve).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    let mut bpm = bpm_at(best as f32 + offset);
    while bpm < TEMPO_MIN {
        bpm *= 2.0;
    }
    while bpm > TEMPO_MAX {
        bpm /= 2.0;
    }
    // A steady sound's onset strength is a faint ripple (the frames' phase against the
    // waves) that can be very regular: its periodicity says nothing without onsets strong
    // enough to hear. Real music averages tens; a sustained tone, a fraction of one.
    let clarity = (mean / ONSET_REFERENCE).min(1.0);
    (bpm, (peak * clarity).clamp(0.0, 1.0))
}

/// How much of each of the twelve pitch classes (C = 0) sounds, from 80 Hz to 4 kHz.
fn chroma(samples: &[f32]) -> [f32; 12] {
    let fft = Fft::new(CHROMA_FRAME);
    let window = hann(CHROMA_FRAME);
    let bin_hz = SAMPLE_RATE as f32 / CHROMA_FRAME as f32;
    let classes: Vec<Option<usize>> = (0..=CHROMA_FRAME / 2)
        .map(|k| {
            let hz = k as f32 * bin_hz;
            (80.0..=4000.0).contains(&hz).then(|| {
                let midi = 69.0 + 12.0 * (hz / 440.0).log2();
                (midi.round() as i32).rem_euclid(12) as usize
            })
        })
        .collect();
    let mut chroma = [0.0f32; 12];
    let mut frame = vec![0.0; CHROMA_FRAME];
    let mut magnitudes = Vec::new();
    let mut start = 0;
    while start + CHROMA_FRAME <= samples.len() {
        let slice = &samples[start..start + CHROMA_FRAME];
        for ((out, sample), w) in frame.iter_mut().zip(slice).zip(&window) {
            *out = sample * w;
        }
        fft.magnitudes(&frame, &mut magnitudes);
        for (magnitude, class) in magnitudes.iter().zip(&classes) {
            if let Some(class) = class {
                chroma[*class] += magnitude * magnitude;
            }
        }
        start += CHROMA_FRAME / 2;
    }
    chroma
}

/// Krumhansl and Kessler's key profiles: how well each pitch class fits a major or minor
/// key on its tonic.
const MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

fn pearson(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let mean_a = a.iter().sum::<f32>() / 12.0;
    let mean_b = b.iter().sum::<f32>() / 12.0;
    let (mut cov, mut var_a, mut var_b) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        cov += (x - mean_a) * (y - mean_b);
        var_a += (x - mean_a).powi(2);
        var_b += (y - mean_b).powi(2);
    }
    if var_a <= 0.0 || var_b <= 0.0 {
        0.0
    } else {
        cov / (var_a * var_b).sqrt()
    }
}

/// The best major key's fit minus the best minor key's (−1 to 1): positive for music that
/// sits in a major key, negative for minor. Relative keys share their notes, so the
/// difference is usually small; the library's own spread of it is what the wave uses.
fn valence(chroma: &[f32; 12]) -> f32 {
    let best = |profile: &[f32; 12]| {
        (0..12)
            .map(|tonic| {
                let mut rotated = [0.0; 12];
                for (class, value) in rotated.iter_mut().enumerate() {
                    *value = profile[(class + 12 - tonic) % 12];
                }
                pearson(chroma, &rotated)
            })
            .fold(f32::MIN, f32::max)
    };
    (best(&MAJOR) - best(&MINOR)).clamp(-1.0, 1.0)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::f32::consts::PI;

    const RATE: f32 = SAMPLE_RATE as f32;

    pub(crate) fn tone(hz: f32, amplitude: f32, seconds: f32) -> Vec<f32> {
        (0..(seconds * RATE) as usize)
            .map(|i| amplitude * (2.0 * PI * hz * i as f32 / RATE).sin())
            .collect()
    }

    fn mix(parts: &[Vec<f32>]) -> Vec<f32> {
        let len = parts.iter().map(Vec::len).min().unwrap_or(0);
        (0..len).map(|i| parts.iter().map(|p| p[i]).sum()).collect()
    }

    /// Short noise bursts every beat over a quiet hum (a deterministic generator).
    pub(crate) fn clicks(bpm: f32, seconds: f32) -> Vec<f32> {
        let period = RATE * 60.0 / bpm;
        let mut state = 0x2545_f491_u32;
        let mut noise = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 2.0 - 1.0
        };
        (0..(seconds * RATE) as usize)
            .map(|i| {
                let since = i as f32 % period;
                let burst = if since < 0.03 * RATE {
                    0.6 * noise() * (1.0 - since / (0.03 * RATE))
                } else {
                    0.0
                };
                burst + 0.05 * (2.0 * PI * 110.0 * i as f32 / RATE).sin()
            })
            .collect()
    }

    #[test]
    fn loudness_follows_the_level() {
        // A sine of amplitude a has a power of a²/2: 0.5 → −9 dBFS.
        let loud = analyze(&tone(440.0, 0.5, 10.0)).unwrap();
        assert!((loud.energy + 9.03).abs() < 0.2, "{}", loud.energy);
        assert!(loud.spread < 0.1);
        let quiet = analyze(&tone(440.0, 0.05, 10.0)).unwrap();
        assert!((quiet.energy + 29.03).abs() < 0.2, "{}", quiet.energy);
    }

    #[test]
    fn music_that_swells_has_spread() {
        let mut swelling = tone(440.0, 0.5, 4.0);
        swelling.extend(tone(440.0, 0.05, 4.0));
        swelling.extend(tone(440.0, 0.5, 4.0));
        swelling.extend(tone(440.0, 0.05, 4.0));
        let features = analyze(&swelling).unwrap();
        assert!(features.spread > 9.0, "{}", features.spread);
    }

    #[test]
    fn bright_sounds_are_bright() {
        let low = analyze(&tone(220.0, 0.3, 8.0)).unwrap();
        let high = analyze(&tone(3000.0, 0.3, 8.0)).unwrap();
        assert!((low.brightness - 220.0).abs() < 30.0, "{}", low.brightness);
        assert!(
            (high.brightness - 3000.0).abs() < 60.0,
            "{}",
            high.brightness
        );
    }

    #[test]
    fn finds_the_beat() {
        for bpm in [92.0, 120.0, 128.0, 150.0, 174.0] {
            let features = analyze(&clicks(bpm, 30.0)).unwrap();
            assert!(
                (features.tempo - bpm).abs() < 2.5,
                "{bpm} BPM measured as {}",
                features.tempo
            );
            assert!(
                features.tempo_confidence > 0.3,
                "{}",
                features.tempo_confidence
            );
        }
        // Half and double time land in the same range.
        let slow = analyze(&clicks(60.0, 30.0)).unwrap();
        assert!((slow.tempo - 120.0).abs() < 3.0, "{}", slow.tempo);
    }

    #[test]
    fn a_steady_tone_has_no_beat() {
        let features = analyze(&tone(330.0, 0.3, 20.0)).unwrap();
        assert!(
            features.tempo_confidence < 0.2,
            "{}",
            features.tempo_confidence
        );
    }

    #[test]
    fn major_and_minor_chords_lean_their_way() {
        // C major (C E G) and C minor (C E♭ G) around middle C.
        let major = mix(&[
            tone(261.63, 0.2, 10.0),
            tone(329.63, 0.2, 10.0),
            tone(392.00, 0.2, 10.0),
        ]);
        let minor = mix(&[
            tone(261.63, 0.2, 10.0),
            tone(311.13, 0.2, 10.0),
            tone(392.00, 0.2, 10.0),
        ]);
        let major = analyze(&major).unwrap().valence;
        let minor = analyze(&minor).unwrap().valence;
        assert!(major > 0.0, "{major}");
        assert!(minor < 0.0, "{minor}");
    }

    #[test]
    fn too_little_to_tell() {
        assert_eq!(
            analyze(&tone(440.0, 0.5, 3.0)),
            Err(AnalysisError::TooShort)
        );
        assert_eq!(
            analyze(&vec![0.0; 10 * SAMPLE_RATE as usize]),
            Err(AnalysisError::Silent)
        );
        assert_eq!(
            analyze(&tone(440.0, 0.0005, 10.0)),
            Err(AnalysisError::Silent)
        );
    }
}
