//! The analysis on audio a real FFmpeg decodes (opt-in: `VITRA_FFMPEG=/path/to/ffmpeg
//! cargo test -p mood --test real_ffmpeg`). FFmpeg makes the test files itself.

use std::path::{Path, PathBuf};
use std::process::Command;

use mood::ffmpeg::{file_args, samples, slices};

fn ffmpeg() -> Option<PathBuf> {
    std::env::var_os("VITRA_FFMPEG").map(PathBuf::from)
}

fn make(ffmpeg: &Path, source: &str, name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("vitra-mood-{}-{name}", std::process::id()));
    let status = Command::new(ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
        ])
        .arg(source)
        .args(["-ac", "2", "-ar", "44100"])
        .arg(&path)
        .status()
        .expect("FFmpeg runs");
    assert!(status.success());
    path
}

fn analyze(ffmpeg: &Path, path: &Path, duration: f64) -> mood::Features {
    let output = Command::new(ffmpeg)
        .args(file_args(path.as_os_str(), &slices(Some(duration))))
        .output()
        .expect("FFmpeg runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pcm = samples(&output.stdout);
    // Three slices of 30 s at the analysis rate.
    assert!(
        (pcm.len() as f64 - 90.0 * 22_050.0).abs() < 22_050.0,
        "{}",
        pcm.len()
    );
    mood::analyze(&pcm).expect("analyzable")
}

#[test]
fn a_tone_and_a_beat_through_ffmpeg() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("VITRA_FFMPEG not set: skipped");
        return;
    };
    // lavfi's sine has an amplitude of 1/8: about −21 dBFS.
    let tone = make(&ffmpeg, "sine=frequency=440:duration=200", "tone.flac");
    let features = analyze(&ffmpeg, &tone, 200.0);
    assert!((features.energy + 21.1).abs() < 0.5, "{}", features.energy);
    assert!(
        (features.brightness - 440.0).abs() < 40.0,
        "{}",
        features.brightness
    );
    std::fs::remove_file(&tone).ok();

    // A 1 kHz ping every half second: 120 BPM.
    let beat = make(
        &ffmpeg,
        "aevalsrc=exp(-mod(t\\,0.5)*40)*sin(2*PI*1000*t):d=200",
        "beat.m4a",
    );
    let features = analyze(&ffmpeg, &beat, 200.0);
    assert!((features.tempo - 120.0).abs() < 2.0, "{}", features.tempo);
    assert!(
        features.tempo_confidence > 0.3,
        "{}",
        features.tempo_confidence
    );
    std::fs::remove_file(&beat).ok();
}
