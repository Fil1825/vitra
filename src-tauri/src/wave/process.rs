//! FFmpeg for the analysis: at the lowest priority the system offers, so playback and
//! everything else come first, and stopped the moment the app quits.

use std::ffi::OsString;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};

/// The most FFmpeg may take for one track (a file on a slow network share, a slow stream).
pub const DECODE_TIMEOUT: Duration = Duration::from_secs(180);

/// `ffmpeg` started through the platform's "background work" door: macOS's background QoS
/// (`taskpolicy -b`: lowest CPU priority, throttled disk, efficiency cores), `nice` elsewhere
/// on Unix, the idle priority class on Windows. Both wrappers exec FFmpeg in their own place,
/// so killing the child kills FFmpeg.
fn low_priority(ffmpeg: &Path) -> Command {
    #[cfg(target_os = "macos")]
    {
        const TASKPOLICY: &str = "/usr/sbin/taskpolicy";
        if Path::new(TASKPOLICY).exists() {
            let mut command = Command::new(TASKPOLICY);
            command.arg("-b").arg(ffmpeg);
            return command;
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for nice in ["/usr/bin/nice", "/bin/nice"] {
            if Path::new(nice).exists() {
                let mut command = Command::new(nice);
                command.args(["-n", "15"]).arg(ffmpeg);
                return command;
            }
        }
    }
    let command = Command::new(ffmpeg);
    #[cfg(windows)]
    let command = {
        let mut command = command;
        const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(IDLE_PRIORITY_CLASS | CREATE_NO_WINDOW);
        command
    };
    command
}

/// The FFmpeg that runs now, so quitting can stop it at once.
#[derive(Default)]
pub struct Running(Mutex<Option<Child>>);

impl Running {
    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Child>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Kills the running FFmpeg, if any.
    pub fn kill(&self) {
        if let Some(child) = self.slot().as_mut()
            && let Err(err) = child.start_kill()
        {
            log::debug!("cannot stop FFmpeg: {err}");
        }
    }
}

/// Runs FFmpeg with `args` and returns what it wrote (raw samples).
pub async fn decode(
    ffmpeg: &Path,
    args: Vec<OsString>,
    running: &Running,
) -> Result<Vec<u8>, String> {
    let mut command = low_priority(ffmpeg);
    links::tls::configure(command.as_std_mut())
        .map_err(|err| format!("FFmpeg failed: TLS setup: {err}"))?;
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|err| format!("FFmpeg failed: cannot run: {err}"))?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err("FFmpeg failed: no output".into());
    };
    *running.slot() = Some(child);
    let mut samples = Vec::new();
    let mut messages = Vec::new();
    let read = async {
        let (out, err) = tokio::join!(
            stdout.read_to_end(&mut samples),
            stderr.read_to_end(&mut messages)
        );
        out.and(err)
    };
    let read = tokio::time::timeout(DECODE_TIMEOUT, read).await;
    let child = running.slot().take();
    let Some(mut child) = child else {
        return Err("stopped".into());
    };
    if !matches!(read, Ok(Ok(_))) {
        let _ = child.start_kill();
    }
    let status = child
        .wait()
        .await
        .map_err(|err| format!("FFmpeg failed: {err}"))?;
    match read {
        Err(_) => Err("FFmpeg failed: decoding took too long".into()),
        Ok(Err(err)) => Err(format!("FFmpeg failed: cannot read output: {err}")),
        Ok(Ok(_)) if status.success() => Ok(samples),
        Ok(Ok(_)) => {
            let messages = String::from_utf8_lossy(&messages);
            Err(failure_message(&messages))
        }
    }
}

/// Keep the cause (TLS, HTTP), which precedes FFmpeg's generic final I/O error.
fn failure_message(messages: &str) -> String {
    let lines: Vec<_> = messages
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = &lines[lines.len().saturating_sub(12)..];
    format!(
        "FFmpeg failed: {}",
        if tail.is_empty() {
            "no message".into()
        } else {
            tail.join("\n")
        }
    )
}

/// Whether `path` is a placeholder whose content lives only in iCloud (an "optimized
/// storage" file): reading it would download it, so the analysis leaves it alone.
#[cfg(target_os = "macos")]
pub fn offloaded(path: &Path) -> bool {
    use std::os::macos::fs::MetadataExt;
    /// `SF_DATALESS` from <sys/stat.h>.
    const DATALESS: u32 = 0x4000_0000;
    std::fs::metadata(path).is_ok_and(|meta| meta.st_flags() & DATALESS != 0)
}

#[cfg(not(target_os = "macos"))]
pub fn offloaded(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[tokio::test]
    async fn ffmpeg_receives_os_roots_without_a_parent_override() {
        // Use a subprocess to remove inherited trust overrides without racing other tests
        // or mutating the test runner's environment (unsafe in Rust 2024).
        const CHILD: &str = "VITRA_TEST_CA_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "wave::process::tests::ffmpeg_receives_os_roots_without_a_parent_override",
                ])
                .env_remove("SSL_CERT_FILE")
                .env_remove("SSL_CERT_DIR")
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let helper = dir.path().join("ffmpeg trust probe");
        std::fs::write(&helper, b"#!/bin/sh\ntest -n \"$SSL_CERT_FILE\" && test -f \"$SSL_CERT_FILE\" || exit 42\nprintf '\\000\\000\\000\\000'\n").unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let output = decode(&helper, vec![], &Running::default()).await.unwrap();
        assert_eq!(output, vec![0; 4]);
    }

    #[test]
    fn tls_cause_survives_the_final_io_error() {
        let message = failure_message(
            "[tls] certificate verify failed\nError opening input files: Input/output error\n",
        );
        assert!(message.contains("certificate verify failed"));
        assert!(message.contains("Input/output error"));
        assert_eq!(failure_message("\n"), "FFmpeg failed: no message");
    }

    #[tokio::test]
    #[ignore = "run scripts/test-ffmpeg-tls.py with a real sidecar"]
    async fn real_https_stream_authenticates_server() {
        use mood::ffmpeg::{Slice, stream_args};

        let ffmpeg = std::env::var_os("VITRA_FFMPEG").expect("VITRA_FFMPEG");
        let url = std::env::var("VITRA_TLS_TEST_URL").expect("VITRA_TLS_TEST_URL");
        let untrusted = std::env::var_os("VITRA_TLS_UNTRUSTED_CA").expect("VITRA_TLS_UNTRUSTED_CA");
        let args = stream_args(
            &url,
            &[],
            Slice {
                start: 0.0,
                seconds: 5.0,
            },
        );
        let decoded = decode(Path::new(&ffmpeg), args.clone(), &Running::default())
            .await
            .unwrap();
        assert_eq!(
            decoded.len(),
            5 * 22_050 * 4,
            "five seconds of mono f32 audio"
        );
        let http_url = std::env::var("VITRA_HTTP_TEST_URL").expect("VITRA_HTTP_TEST_URL");
        let http_args = stream_args(
            &http_url,
            &[],
            Slice {
                start: 0.0,
                seconds: 5.0,
            },
        );
        let plain = decode(Path::new(&ffmpeg), http_args, &Running::default())
            .await
            .unwrap();
        assert_eq!(
            plain.len(),
            decoded.len(),
            "plain HTTP radio must still work"
        );

        let mut command = std::process::Command::new(ffmpeg);
        command
            .args(args)
            .env("SSL_CERT_FILE", untrusted)
            .env_remove("SSL_CERT_DIR");
        links::tls::configure(&mut command).unwrap();
        let rejected = command.output().unwrap();
        assert!(
            !rejected.status.success(),
            "an untrusted certificate must be rejected"
        );
        let stderr = String::from_utf8_lossy(&rejected.stderr).to_lowercase();
        assert!(stderr.contains("certificate"), "{stderr}");
    }

    #[tokio::test]
    #[ignore = "needs real yt-dlp, FFmpeg, Deno, and access to VITRA_STREAM_URL"]
    async fn real_ytdlp_https_stream() {
        use mood::ffmpeg::{Slice, stream_args};

        let runner = links::YtDlp {
            binary: std::env::var_os("VITRA_YTDLP").expect("VITRA_YTDLP").into(),
            deno: Some(std::env::var_os("VITRA_DENO").expect("VITRA_DENO").into()),
            ..links::YtDlp::default()
        };
        let url = std::env::var("VITRA_STREAM_URL").expect("VITRA_STREAM_URL");
        let scratch = tempfile::tempdir().unwrap();
        let stream = runner.audio_stream(&url, scratch.path()).await.unwrap();
        assert!(stream.url.starts_with("https://"));
        let ffmpeg = std::env::var_os("VITRA_FFMPEG").expect("VITRA_FFMPEG");
        let args = stream_args(
            &stream.url,
            &stream.headers,
            Slice {
                start: 0.0,
                seconds: 5.0,
            },
        );
        let decoded = decode(Path::new(&ffmpeg), args, &Running::default())
            .await
            .unwrap();
        assert!(
            !decoded.is_empty(),
            "the real stream must yield audio samples"
        );
    }
}
