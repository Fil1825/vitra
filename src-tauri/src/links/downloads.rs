//! The download queue: links downloaded into the library a few at a time, with progress,
//! cancelling and retrying. A finished download takes over its streamed track's row.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use library::Track;
use links::{DownloadEvent, DownloadOptions, DownloadStatus, FileNameFields, LinkError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use super::LinkService;
use crate::error::AppError;
use crate::library::{Change, LibraryService};
use crate::player::Player;
use crate::settings::{DownloadSettings, SettingsStore};

pub const DOWNLOADS_EVENT: &str = "downloads://changed";

/// Downloads running at once: enough to keep the line busy, few enough not to get
/// throttled by the site.
const PARALLEL: usize = 2;
/// Progress is sent to the windows at most this often.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// One download as the windows see it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub track_id: i64,
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(skip)]
    year: Option<i64>,
    pub status: DownloadStatus,
    /// Stops the running yt-dlp.
    #[serde(skip)]
    cancel: Option<watch::Sender<bool>>,
    /// The file name (without extension) this job reserved.
    #[serde(skip)]
    stem: Option<String>,
}

#[derive(Clone)]
pub struct Downloads(Arc<Inner>);

struct Inner {
    app: AppHandle,
    jobs: Mutex<Vec<Job>>,
    next_id: AtomicU64,
    /// Partial files, one directory per job.
    temp_root: PathBuf,
    last_progress: Mutex<Instant>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The folder downloads go to: the chosen one, else `Vitra` in the user's music folder.
pub fn download_folder(app: &AppHandle, settings: &DownloadSettings) -> Result<PathBuf, AppError> {
    if !settings.folder.trim().is_empty() {
        return Ok(PathBuf::from(settings.folder.trim()));
    }
    let music = app
        .path()
        .audio_dir()
        .or_else(|_| app.path().home_dir().map(|home| home.join("Music")))?;
    Ok(music.join("Vitra"))
}

/// File names (without extension, lowercased) in `dir`: a new download must not take one.
fn stems_in(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            Path::new(&entry.file_name())
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_lowercase())
        })
        .collect()
}

impl Downloads {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        let temp_root = app.path().app_cache_dir()?.join("downloads");
        // Partial files of downloads cut short by a crash or quitting.
        let _ = std::fs::remove_dir_all(&temp_root);
        Ok(Self(Arc::new(Inner {
            app,
            jobs: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            temp_root,
            last_progress: Mutex::new(Instant::now()),
        })))
    }

    pub fn jobs(&self) -> Vec<Job> {
        lock(&self.0.jobs).clone()
    }

    fn emit(&self) {
        *lock(&self.0.last_progress) = Instant::now();
        if let Err(err) = self.0.app.emit(DOWNLOADS_EVENT, self.jobs()) {
            log::warn!("cannot emit {DOWNLOADS_EVENT}: {err}");
        }
    }

    fn emit_progress(&self) {
        if lock(&self.0.last_progress).elapsed() >= PROGRESS_INTERVAL {
            self.emit();
        }
    }

    /// Changes a job's status if the transition is allowed; returns whether it changed.
    fn transition(
        &self,
        id: u64,
        next: impl FnOnce(&DownloadStatus) -> Option<DownloadStatus>,
    ) -> bool {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        match next(&job.status) {
            Some(status) if status != job.status => {
                job.status = status;
                true
            }
            _ => false,
        }
    }

    /// Queues link tracks for download. Tracks that are files already, or queued already,
    /// are skipped. Returns how many were queued.
    pub fn add(&self, tracks: Vec<Track>) -> usize {
        let mut added = 0;
        {
            let mut jobs = lock(&self.0.jobs);
            for track in tracks {
                let Some(url) = track.source_url.clone() else {
                    continue;
                };
                if !links::is_remote(&track.path) {
                    continue;
                }
                let active = jobs
                    .iter()
                    .any(|job| job.track_id == track.id && job.status.is_active());
                if active {
                    continue;
                }
                // A failed or cancelled attempt of the same track makes room for this one.
                jobs.retain(|job| job.track_id != track.id);
                jobs.push(Job {
                    id: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                    track_id: track.id,
                    url,
                    title: track.title,
                    artist: track.artist,
                    album: track.album,
                    year: track.year,
                    status: DownloadStatus::Queued,
                    cancel: None,
                    stem: None,
                });
                added += 1;
            }
        }
        if added > 0 {
            self.emit();
            self.pump();
        }
        added
    }

    pub fn cancel(&self, id: u64) {
        let cancel = {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
                return;
            };
            if job.status == DownloadStatus::Queued {
                job.status = DownloadStatus::Cancelled;
                None
            } else {
                job.cancel.clone()
            }
        };
        match cancel {
            // The running job reports itself cancelled once yt-dlp is gone.
            Some(cancel) => {
                let _ = cancel.send(true);
            }
            None => self.emit(),
        }
    }

    pub fn retry(&self, id: u64) {
        if self.transition(id, DownloadStatus::retried) {
            self.emit();
            self.pump();
        }
    }

    /// Forgets finished, failed and cancelled downloads.
    pub fn clear(&self) {
        lock(&self.0.jobs).retain(|job| job.status.is_active());
        self.emit();
    }

    /// Starts queued jobs while there are free slots.
    fn pump(&self) {
        loop {
            let started = {
                let mut jobs = lock(&self.0.jobs);
                let running = jobs
                    .iter()
                    .filter(|job| job.status.is_active() && job.status != DownloadStatus::Queued)
                    .count();
                if running >= PARALLEL {
                    return;
                }
                let Some(job) = jobs
                    .iter_mut()
                    .find(|job| job.status == DownloadStatus::Queued)
                else {
                    return;
                };
                let Some(status) = job.status.started() else {
                    return;
                };
                job.status = status;
                let (sender, receiver) = watch::channel(false);
                job.cancel = Some(sender);
                (job.clone(), receiver)
            };
            self.emit();
            let (job, cancel) = started;
            let downloads = self.clone();
            tauri::async_runtime::spawn(async move {
                let id = job.id;
                let result = downloads.run(job, cancel).await;
                downloads.finish(id, result);
                downloads.pump();
            });
        }
    }

    fn finish(&self, id: u64, result: Result<String, AppError>) {
        {
            let mut jobs = lock(&self.0.jobs);
            if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
                job.cancel = None;
                job.stem = None;
                let next = match result {
                    Ok(path) => job.status.finished(path),
                    Err(AppError::Link(LinkError::Cancelled)) => job.status.cancelled(),
                    Err(err) => {
                        log::warn!("downloading {} failed: {err}", job.url);
                        job.status.failed(err.to_string())
                    }
                };
                if let Some(status) = next {
                    job.status = status;
                }
            }
        }
        let _ = std::fs::remove_dir_all(self.0.temp_root.join(id.to_string()));
        self.emit();
    }

    /// Reserves a file name no file in `dir` and no other download has.
    fn reserve_stem(&self, id: u64, dir: &Path, wanted: &str) -> String {
        let on_disk = stems_in(dir);
        let mut jobs = lock(&self.0.jobs);
        let reserved: HashSet<String> = jobs
            .iter()
            .filter_map(|job| job.stem.as_deref().map(str::to_lowercase))
            .collect();
        let stem = links::unique_stem(wanted, |candidate| {
            let key = candidate.to_lowercase();
            on_disk.contains(&key) || reserved.contains(&key)
        });
        if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            job.stem = Some(stem.clone());
        }
        stem
    }

    /// Downloads one job's link, tags the file and puts it in the library; returns the file.
    async fn run(&self, job: Job, cancel: watch::Receiver<bool>) -> Result<String, AppError> {
        let app = &self.0.app;
        let links = app.state::<LinkService>().inner().clone();
        let library = app.state::<LibraryService>().inner().clone();
        let settings = app.state::<SettingsStore>().get().downloads;

        let ytdlp = links.ytdlp()?;
        if ytdlp.ffmpeg.is_none() {
            return Err(AppError::NotFound(
                "FFmpeg is needed for downloads but was not found".into(),
            ));
        }
        let dir = download_folder(app, &settings)?;
        let year = job.year.and_then(|year| u16::try_from(year).ok());
        let wanted = links::file_stem(
            &settings.template,
            &FileNameFields {
                artist: job.artist.as_deref(),
                title: Some(&job.title),
                album: job.album.as_deref(),
                year,
            },
        );
        let options = DownloadOptions {
            format: settings.format,
            mp3_bitrate: settings.mp3_bitrate,
            file_stem: self.reserve_stem(job.id, &dir, &wanted),
            dir,
            temp_dir: self.0.temp_root.join(job.id.to_string()),
            ffmpeg: ytdlp.ffmpeg.clone(),
        };

        let id = job.id;
        let file = ytdlp
            .download(&job.url, &options, cancel, |event| {
                if self.transition(id, |status| status.on_event(event)) {
                    self.emit();
                } else if matches!(event, DownloadEvent::Progress { .. }) {
                    self.emit_progress();
                }
            })
            .await?;
        // Tagging and moving into the library are part of processing.
        if self.transition(id, |status| {
            status.on_event(&DownloadEvent::Processing("Vitra".into()))
        }) {
            self.emit();
        }

        let track = {
            let library = library.clone();
            let job = job.clone();
            let file = file.clone();
            tauri::async_runtime::spawn_blocking(move || {
                tag_file(&library, &job, year, &file);
                library.with(|db| db.attach_download(job.track_id, &file, library.covers_dir()))
            })
            .await??
        };
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        library.changed(Change::Folders);
        if let Some(player) = app.try_state::<Player>() {
            player.link_downloaded(&job.url, &track);
        }
        Ok(track.path)
    }
}

/// Writes the track's metadata as Vitra shows it (cleaner than the page's title and
/// uploader yt-dlp embeds) and its lyrics into the file. The cover yt-dlp embedded stays.
fn tag_file(library: &LibraryService, job: &Job, year: Option<u16>, file: &Path) {
    let lyrics = library
        .with(|db| db.lyrics_text(job.track_id))
        .ok()
        .flatten();
    let album_artist = media::read_track(file)
        .ok()
        .and_then(|info| info.metadata.album_artist)
        .unwrap_or_default();
    let values = media::TagValues {
        title: job.title.clone(),
        artist: job.artist.clone().unwrap_or_default(),
        album: job.album.clone().unwrap_or_default(),
        album_artist,
        year,
        lyrics,
    };
    if let Err(err) = media::write_tags(file, &values, &media::CoverChange::Keep) {
        log::warn!("cannot tag {}: {err}", file.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_files_are_found_by_stem_ignoring_case() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Artist - Song.opus"), b"").unwrap();
        std::fs::write(tmp.path().join("Other.mp3"), b"").unwrap();
        let stems = stems_in(tmp.path());
        assert!(stems.contains("artist - song"));
        assert!(stems.contains("other"));
        assert!(stems_in(&tmp.path().join("missing")).is_empty());
    }
}
