//! The music library service: owns the database, scans folders on a background thread and
//! watches them for changes.
//!
//! Commands query through one shared connection; the scanner writes through its own. SQLite
//! runs in WAL mode, so a long scan doesn't block the UI.

mod commands;
mod watch;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use library::{Library, ScanPhase, ScanProgress, Track};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::player::Entry;

pub use commands::*;

pub const CHANGED_EVENT: &str = "library://changed";
pub const SCAN_EVENT: &str = "library://scan";

/// While a long scan runs, the views refresh this often so the library fills up visibly.
const LIVE_REFRESH: Duration = Duration::from_secs(2);

/// What changed, so each view refetches only what it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    /// Tracks, albums or artists (including favorites and play counts).
    Tracks,
    Playlists,
    Folders,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanStatus {
    pub scanning: bool,
    pub folder_id: Option<i64>,
    pub phase: Option<ScanPhase>,
    pub done: usize,
    pub total: usize,
    /// Folders waiting for their turn.
    pub queued: usize,
}

#[derive(Clone)]
pub struct LibraryService(Arc<Inner>);

struct Inner {
    app: AppHandle,
    db_path: PathBuf,
    covers_dir: PathBuf,
    db: Mutex<Library>,
    scan_requests: Mutex<Sender<i64>>,
    status: Mutex<ScanStatus>,
    /// Cancels the running scan (its folder was removed, or the app quits).
    cancel: Mutex<Arc<AtomicBool>>,
    watcher: Mutex<Option<watch::FolderWatcher>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Opens the database; a file that can't be opened is set aside and a fresh one created, so
/// a damaged library never keeps the app from starting.
fn open_database(path: &Path) -> Result<Library, AppError> {
    match Library::open(path) {
        Ok(library) => Ok(library),
        Err(err) => {
            log::error!("cannot open the library at {}: {err}", path.display());
            let aside = path.with_extension(format!("broken-{}.db", library::now()));
            std::fs::rename(path, &aside)?;
            log::warn!("moved the damaged library to {}", aside.display());
            Ok(Library::open(path)?)
        }
    }
}

impl LibraryService {
    /// Opens the library in `data_dir` and starts the scanner thread.
    pub fn start(app: AppHandle, data_dir: &Path) -> Result<Self, AppError> {
        let db_path = data_dir.join("library.db");
        let db = open_database(&db_path)?;
        let (sender, receiver) = mpsc::channel();
        let service = Self(Arc::new(Inner {
            app,
            covers_dir: data_dir.join("covers"),
            db_path,
            db: Mutex::new(db),
            scan_requests: Mutex::new(sender),
            status: Mutex::new(ScanStatus::default()),
            cancel: Mutex::new(Arc::new(AtomicBool::new(false))),
            watcher: Mutex::new(None),
        }));

        let scanner = service.clone();
        std::thread::Builder::new()
            .name("library-scan".into())
            .spawn(move || scanner.scan_loop(receiver))?;

        let folders = service.with(|db| db.folders())?;
        *lock(&service.0.watcher) = watch::FolderWatcher::start(service.clone())
            .inspect_err(|err| log::warn!("folder watching is unavailable: {err}"))
            .ok();
        for folder in folders {
            service.watch(&folder.path);
            // Catch up with whatever changed while the app was closed.
            service.request_scan(folder.id);
        }
        Ok(service)
    }

    pub fn covers_dir(&self) -> &Path {
        &self.0.covers_dir
    }

    /// Runs `f` on the shared connection (blocking: call from a blocking context).
    pub fn with<T>(
        &self,
        f: impl FnOnce(&mut Library) -> library::Result<T>,
    ) -> Result<T, AppError> {
        Ok(f(&mut lock(&self.0.db))?)
    }

    /// Runs `f` on the shared connection off the async runtime.
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Library) -> library::Result<T> + Send + 'static,
    ) -> Result<T, AppError> {
        let service = self.clone();
        tauri::async_runtime::spawn_blocking(move || service.with(f)).await?
    }

    pub fn changed(&self, change: Change) {
        if let Err(err) = self.0.app.emit(CHANGED_EVENT, change) {
            log::warn!("cannot emit {CHANGED_EVENT}: {err}");
        }
    }

    pub fn scan_status(&self) -> ScanStatus {
        lock(&self.0.status).clone()
    }

    fn set_status(&self, update: impl FnOnce(&mut ScanStatus)) {
        let status = {
            let mut status = lock(&self.0.status);
            update(&mut status);
            status.clone()
        };
        if let Err(err) = self.0.app.emit(SCAN_EVENT, status) {
            log::warn!("cannot emit {SCAN_EVENT}: {err}");
        }
    }

    pub fn request_scan(&self, folder_id: i64) {
        self.set_status(|status| status.queued += 1);
        let _ = lock(&self.0.scan_requests).send(folder_id);
    }

    fn watch(&self, path: &str) {
        if let Some(watcher) = lock(&self.0.watcher).as_mut() {
            watcher.watch(Path::new(path));
        }
    }

    fn unwatch(&self, path: &str) {
        if let Some(watcher) = lock(&self.0.watcher).as_mut() {
            watcher.unwatch(Path::new(path));
        }
    }

    /// Adds a folder, starts watching it and queues its first scan.
    pub fn add_folder(&self, path: &str) -> Result<library::Folder, AppError> {
        let folder = self.with(|db| db.add_folder(path))?;
        // Child folders merged into this one are covered by its recursive watch now.
        self.watch(&folder.path);
        self.request_scan(folder.id);
        self.changed(Change::Folders);
        Ok(folder)
    }

    pub fn remove_folder(&self, id: i64) -> Result<(), AppError> {
        let Some(folder) = self.with(|db| db.folder(id))? else {
            return Ok(());
        };
        if self.scan_status().folder_id == Some(id) {
            lock(&self.0.cancel).store(true, Ordering::Relaxed);
        }
        self.unwatch(&folder.path);
        self.with(|db| {
            db.remove_folder(id)?;
            db.remove_unused_covers(&self.0.covers_dir)
        })?;
        self.changed(Change::Folders);
        self.changed(Change::Tracks);
        self.changed(Change::Playlists);
        Ok(())
    }

    /// Stops a running scan; called when the app quits.
    pub fn stop(&self) {
        lock(&self.0.cancel).store(true, Ordering::Relaxed);
    }

    /// The scanner thread: scans requested folders one at a time with its own connection.
    fn scan_loop(&self, requests: Receiver<i64>) {
        let mut db = match Library::open(&self.0.db_path) {
            Ok(db) => db,
            Err(err) => {
                log::error!("the scanner cannot open the library: {err}");
                return;
            }
        };
        let mut pending: VecDeque<i64> = VecDeque::new();
        loop {
            // Collect every request that's waiting, dropping duplicates.
            let first = if pending.is_empty() {
                match requests.recv() {
                    Ok(id) => Some(id),
                    Err(_) => return,
                }
            } else {
                None
            };
            for id in first.into_iter().chain(requests.try_iter()) {
                if !pending.contains(&id) {
                    pending.push_back(id);
                }
            }
            let Some(folder_id) = pending.pop_front() else {
                continue;
            };

            let cancel = Arc::new(AtomicBool::new(false));
            *lock(&self.0.cancel) = Arc::clone(&cancel);
            let queued = pending.len();
            self.set_status(|status| {
                *status = ScanStatus {
                    scanning: true,
                    folder_id: Some(folder_id),
                    queued,
                    ..ScanStatus::default()
                };
            });

            let mut last_refresh = Instant::now();
            let mut progress = |p: ScanProgress| {
                self.set_status(|status| {
                    status.phase = Some(p.phase);
                    status.done = p.done;
                    status.total = p.total;
                });
                if p.phase == ScanPhase::Reading && last_refresh.elapsed() >= LIVE_REFRESH {
                    last_refresh = Instant::now();
                    self.changed(Change::Tracks);
                }
            };
            match db.scan_folder(folder_id, &self.0.covers_dir, &cancel, &mut progress) {
                Ok(summary) => {
                    if summary.changed() {
                        self.changed(Change::Tracks);
                        self.changed(Change::Playlists);
                    }
                    self.changed(Change::Folders);
                }
                Err(err) => log::warn!("scanning folder {folder_id} failed: {err}"),
            }
            let queued = pending.len();
            self.set_status(|status| {
                *status = ScanStatus {
                    queued,
                    ..ScanStatus::default()
                };
            });
        }
    }

    /// Re-reads tags of files after they were edited.
    pub fn refresh_files(&self, paths: &[&Path]) -> Result<(), AppError> {
        self.with(|db| db.refresh_files(paths, &self.0.covers_dir))?;
        self.changed(Change::Tracks);
        self.changed(Change::Playlists);
        Ok(())
    }

    /// Queue entries for library tracks, in the given order; unknown ids are skipped.
    pub async fn entries(&self, track_ids: Vec<i64>) -> Result<Vec<Entry>, AppError> {
        let tracks = self
            .run(move |db| db.tracks(&library::TrackFilter::Ids(track_ids)))
            .await?;
        Ok(tracks.into_iter().map(entry_from_track).collect())
    }

    /// Counts a play of a library track (in the background).
    pub fn record_play(&self, track_id: i64) {
        let service = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            match service.with(|db| db.record_play(track_id, library::now())) {
                Ok(()) => service.changed(Change::Tracks),
                Err(err) => log::warn!("cannot record a play: {err}"),
            }
        });
    }
}

pub fn entry_from_track(track: Track) -> Entry {
    Entry {
        path: track.path,
        track_id: Some(track.id),
        title: track.title,
        artist: track.artist,
        album: track.album,
        duration: track.duration,
        cover: track.cover,
    }
}
