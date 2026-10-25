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
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|err| format!("cannot run FFmpeg: {err}"))?;
    let (Some(mut stdout), Some(mut stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err("FFmpeg has no output".into());
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
        Err(_) => Err("FFmpeg took too long".into()),
        Ok(Err(err)) => Err(format!("cannot read FFmpeg's output: {err}")),
        Ok(Ok(_)) if status.success() => Ok(samples),
        Ok(Ok(_)) => {
            let messages = String::from_utf8_lossy(&messages);
            let last = messages.lines().rev().find(|line| !line.trim().is_empty());
            Err(format!(
                "FFmpeg failed: {}",
                last.map_or("no message", str::trim)
            ))
        }
    }
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
