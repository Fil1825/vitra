//! Talks to a real mpv. Opt-in: `VITRA_MPV_TESTS=1 cargo test -p mpv-ipc` (needs `mpv` on PATH
//! or `VITRA_MPV=/path/to/mpv`). Without it the test passes without doing anything.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mpv_ipc::{Error, Event, MpvProcess, SpawnOptions};
use serde_json::json;
use tokio::time::timeout;

fn mpv_binary() -> Option<PathBuf> {
    if std::env::var_os("VITRA_MPV_TESTS").is_none() {
        eprintln!("skipped: set VITRA_MPV_TESTS=1 to run against a real mpv");
        return None;
    }
    if let Some(path) = std::env::var_os("VITRA_MPV") {
        return Some(path.into());
    }
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(if cfg!(windows) { "mpv.exe" } else { "mpv" }))
        .find(|path| path.is_file())
}

/// One second of a 440 Hz sine as 16-bit mono PCM WAV.
fn write_sine_wav(path: &Path) {
    let rate = 8_000u32;
    let samples: Vec<i16> = (0..rate)
        .map(|i| {
            let t = f64::from(i) / f64::from(rate);
            ((t * 440.0 * std::f64::consts::TAU).sin() * 8_000.0) as i16
        })
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, wav).unwrap();
}

async fn wait_for(events: &mut mpv_ipc::Events, want: impl Fn(&Event) -> bool) -> Event {
    timeout(Duration::from_secs(10), async {
        loop {
            let event = events.recv().await.expect("mpv closed the connection");
            if want(&event) {
                return event;
            }
        }
    })
    .await
    .expect("timed out waiting for an mpv event")
}

#[tokio::test(flavor = "multi_thread")]
async fn plays_seeks_and_quits() {
    let Some(binary) = mpv_binary() else { return };
    let dir = std::env::temp_dir().join(format!("mpv-ipc-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("sine.wav");
    write_sine_wav(&wav);

    let (mpv, client, mut events) = MpvProcess::spawn(SpawnOptions {
        binary,
        args: [
            "--idle=yes",
            "--no-config",
            "--no-terminal",
            "--no-video",
            "--ao=null",
        ]
        .map(Into::into)
        .to_vec(),
        pipe_prefix: "mpv-ipc-test".into(),
        startup_timeout: Duration::from_secs(10),
    })
    .await
    .expect("mpv starts");

    assert!(mpv.version().starts_with("mpv"), "{}", mpv.version());

    client.set_property("pause", true).await.unwrap();
    client.observe_property(1, "duration").await.unwrap();
    client
        .command(vec![json!("loadfile"), json!(wav.to_string_lossy())])
        .await
        .unwrap();
    wait_for(&mut events, |e| matches!(e, Event::FileLoaded)).await;

    let duration: f64 = client.get_property("duration").await.unwrap();
    assert!((duration - 1.0).abs() < 0.05, "duration {duration}");

    client
        .command(vec![json!("seek"), json!(0.5), json!("absolute")])
        .await
        .unwrap();
    wait_for(&mut events, |e| matches!(e, Event::PlaybackRestart)).await;
    let position: f64 = client.get_property("time-pos").await.unwrap();
    assert!((position - 0.5).abs() < 0.05, "position {position}");

    let err = client.get_property::<f64>("no-such-property").await;
    assert!(err.is_err());

    mpv.shutdown(&client, Duration::from_secs(5)).await;
    std::fs::remove_dir_all(&dir).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn rejected_options_report_the_exit() {
    let Some(binary) = mpv_binary() else { return };
    let result = MpvProcess::spawn(SpawnOptions {
        binary,
        args: ["--idle=yes", "--no-config", "--no-such-option-in-any-mpv"]
            .map(Into::into)
            .to_vec(),
        pipe_prefix: "mpv-ipc-test-bad".into(),
        startup_timeout: Duration::from_secs(10),
    })
    .await;
    assert!(
        matches!(result, Err(Error::Exited(_))),
        "{:?}",
        result.err()
    );
}

/// The point of the socket pair: if the app goes away, so does mpv.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn mpv_quits_when_the_connection_closes() {
    let Some(binary) = mpv_binary() else { return };
    let (mut mpv, client, events) = MpvProcess::spawn(SpawnOptions {
        binary,
        args: [
            "--idle=yes",
            "--no-config",
            "--no-terminal",
            "--no-video",
            "--ao=null",
        ]
        .map(Into::into)
        .to_vec(),
        pipe_prefix: "mpv-ipc-test-close".into(),
        startup_timeout: Duration::from_secs(10),
    })
    .await
    .expect("mpv starts");

    drop(client);
    drop(events);

    let status = timeout(Duration::from_secs(5), mpv.wait())
        .await
        .expect("mpv exits after the connection closes")
        .unwrap();
    assert!(status.success(), "{status}");
}
