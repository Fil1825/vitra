//! Measuring files' loudness in the background, one at a time: FFmpeg (the sidecar that
//! yt-dlp uses too) decodes the file once through its EBU R128 filter, which takes about a
//! second for a song. The player asks for the tracks about to play, so a file is usually
//! measured before its first note.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use library::{FileStamp, Loudness};
use tauri::{AppHandle, Manager};

use super::loudness::parse_ebur128;
use crate::library::LibraryService;
use crate::player::Player;
use crate::sidecar::{self, Sidecar};

/// Files waiting at most; older wishes give way to newer ones.
const MAX_QUEUED: usize = 8;

/// A file's size and modification time, which tell a stored measurement is still good.
pub fn file_stamp(path: &Path) -> Option<FileStamp> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some((meta.len() as i64, mtime as i64))
}

struct Inner {
    app: AppHandle,
    queue: Mutex<VecDeque<PathBuf>>,
    running: AtomicBool,
}

#[derive(Clone)]
pub struct LoudnessMeter(Arc<Inner>);

impl LoudnessMeter {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(Inner {
            app,
            queue: Mutex::new(VecDeque::new()),
            running: AtomicBool::new(false),
        }))
    }

    /// Measures `path` soon, unless it was measured already (newest wishes first).
    pub fn want(&self, path: &Path) {
        {
            let mut queue = self.lock();
            queue.retain(|queued| queued != path);
            queue.push_front(path.to_path_buf());
            queue.truncate(MAX_QUEUED);
        }
        if !self.0.running.swap(true, Ordering::AcqRel) {
            let meter = self.clone();
            tauri::async_runtime::spawn(async move { meter.work().await });
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<PathBuf>> {
        self.0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn work(self) {
        loop {
            let next = self.lock().pop_front();
            let Some(path) = next else {
                self.0.running.store(false, Ordering::Release);
                // A wish that came in just now.
                if !self.lock().is_empty() && !self.0.running.swap(true, Ordering::AcqRel) {
                    continue;
                }
                return;
            };
            self.measure(path).await;
        }
    }

    async fn measure(&self, path: PathBuf) {
        let Some(library) = self
            .0
            .app
            .try_state::<LibraryService>()
            .map(|l| l.inner().clone())
        else {
            return;
        };
        let Some(stamp) = file_stamp(&path) else {
            return;
        };
        let key = path.to_string_lossy().into_owned();
        let known = {
            let key = key.clone();
            library.run(move |db| db.loudness(&key, Some(stamp))).await
        };
        if matches!(known, Ok(Some(_))) {
            return;
        }
        let Some(ffmpeg) = sidecar::search_paths(&self.0.app).locate(Sidecar::Ffmpeg) else {
            log::info!("loudness: no FFmpeg to measure with");
            return;
        };
        let file = path.clone();
        let measured =
            tauri::async_runtime::spawn_blocking(move || run_ffmpeg(&ffmpeg.path, &file)).await;
        let loudness = match measured {
            Ok(Ok(loudness)) => loudness,
            Ok(Err(err)) => {
                log::info!("loudness of {}: {err}", path.display());
                return;
            }
            Err(err) => {
                log::warn!("loudness of {}: {err}", path.display());
                return;
            }
        };
        let stored = {
            let key = key.clone();
            library
                .run(move |db| db.set_loudness(&key, Some(stamp), loudness))
                .await
        };
        if let Err(err) = stored {
            log::warn!("cannot store the loudness of {}: {err}", path.display());
            return;
        }
        log::info!(
            "loudness of {}: {:.1} LUFS, peak {:?}",
            path.display(),
            loudness.integrated,
            loudness.peak
        );
        if let Some(player) = self.0.app.try_state::<Player>() {
            player.loudness_measured(&key);
        }
    }
}

/// Decodes the first audio stream through `ebur128` and reads its summary.
fn run_ffmpeg(ffmpeg: &Path, path: &Path) -> Result<Loudness, String> {
    let output = Command::new(ffmpeg)
        .args([
            "-nostdin",
            "-hide_banner",
            "-nostats",
            "-threads",
            "1",
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "ebur128=peak=true",
            "-f",
            "null",
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| err.to_string())?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let last = stderr.lines().last().unwrap_or_default();
        return Err(format!("FFmpeg failed: {last}"));
    }
    parse_ebur128(&stderr).ok_or_else(|| "no loudness in FFmpeg's output".into())
}
