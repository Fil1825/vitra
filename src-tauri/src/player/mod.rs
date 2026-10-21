//! The player: runs mpv, mirrors its state, owns the play queue and exposes playback
//! commands.
//!
//! A supervisor task owns the mpv process. It restarts mpv if it dies (restoring volume,
//! speed, the queue and the position) and forwards mpv events through [`PlayerState::apply`]
//! to the windows as `player://state` and throttled `player://position` events.
//!
//! Gapless playback: mpv's own playlist holds the playing file plus the item that follows it
//! in the queue (`loadfile <next> append`), so mpv opens the next file before the current
//! one ends. Every queue change re-mirrors that one entry.

mod artwork;
mod commands;
mod queue;
mod session;
mod state;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use media::TrackMetadata;
use mpv_ipc::{Client, EndReason, Event, MpvProcess, SpawnOptions};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::error::AppError;
use crate::library::{LibraryService, entry_from_track};
use crate::sidecar::{self, Sidecar};

pub use artwork::{
    ArtworkStore, COVER_PREFIX, SCHEME as ARTWORK_SCHEME, respond_cover, status as http_status,
};
pub use commands::*;
pub use queue::{Entry, QueueView, Repeat};
use queue::{Queue, QueueItem};
pub use state::PlayerState;
use state::{ArtworkInfo, OBSERVED, PlayerError, Status, Track};

pub const STATE_EVENT: &str = "player://state";
pub const POSITION_EVENT: &str = "player://position";
pub const TRACK_EVENT: &str = "player://track-changed";
pub const QUEUE_EVENT: &str = "queue://changed";

/// Previous restarts the track instead of going back once it has played this long.
const RESTART_THRESHOLD: f64 = 3.0;
/// Unplayable files in a row that are skipped before playback stops.
const MAX_SKIPS: usize = 5;
/// A play counts once half the track (at most four minutes) has been heard.
const COUNT_AFTER_MAX: f64 = 240.0;
/// Opening a folder queues at most this many files.
const MAX_OPEN: usize = 10_000;

/// The UI interpolates between position updates, so a few per second is enough.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a command waits for mpv to (re)start before giving up.
const READY_TIMEOUT: Duration = Duration::from_secs(15);
/// Give up restarting after this many failures within [`RESTART_WINDOW`].
const MAX_FAILURES: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionUpdate {
    position: f64,
    duration: Option<f64>,
}

#[derive(Clone)]
pub struct Player(Arc<Inner>);

/// Which queue items mpv's playlist holds, by mpv playlist entry id.
#[derive(Debug, Default)]
struct Mirror {
    /// The playing file: (mpv entry id, queue uid).
    current: Option<(i64, u64)>,
    /// The preloaded next file.
    next: Option<(i64, u64)>,
    /// Files that failed to play in a row.
    failures: usize,
    /// Whether the current playback was counted as a play.
    counted: bool,
}

struct Inner {
    app: AppHandle,
    state: Mutex<PlayerState>,
    client: RwLock<Option<Arc<Client>>>,
    artwork: ArtworkStore,
    next_track_id: AtomicU64,
    last_position_emit: Mutex<Instant>,
    /// Restore this position once the reloaded file is ready (session restore, mpv restart).
    pending_seek: Mutex<Option<f64>>,
    /// Signalled whenever a new mpv connection becomes available.
    ready: Notify,
    shutdown: Notify,
    supervisor: Mutex<Option<JoinHandle<()>>>,
    queue: Mutex<Queue>,
    mirror: Mutex<Mirror>,
    /// Serializes operations that change what plays, so two quick clicks can't interleave
    /// their `loadfile` commands and leave mpv's playlist out of step with the queue.
    transport: tokio::sync::Mutex<()>,
    /// The queue changed since the session was last saved.
    queue_dirty: AtomicBool,
    /// The playback values last saved (JSON), to skip writing unchanged ones.
    saved_playback: Mutex<String>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Player {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(Inner {
            app,
            state: Mutex::new(PlayerState::default()),
            client: RwLock::new(None),
            artwork: ArtworkStore::default(),
            next_track_id: AtomicU64::new(1),
            last_position_emit: Mutex::new(Instant::now()),
            pending_seek: Mutex::new(None),
            ready: Notify::new(),
            shutdown: Notify::new(),
            supervisor: Mutex::new(None),
            queue: Mutex::new(Queue::default()),
            mirror: Mutex::new(Mirror::default()),
            transport: tokio::sync::Mutex::new(()),
            queue_dirty: AtomicBool::new(false),
            saved_playback: Mutex::new(String::new()),
        }))
    }

    /// Restores the last session and starts mpv in the background.
    pub fn start(&self) {
        self.restore_session();
        let handle = tauri::async_runtime::spawn(supervise(self.clone()));
        *lock(&self.0.supervisor) = Some(handle);
        tauri::async_runtime::spawn(session::autosave(self.clone()));
    }

    /// Saves the session and quits mpv; called when the app exits so no orphaned mpv keeps
    /// running.
    pub async fn shutdown(&self) {
        self.save_session(true);
        self.0.shutdown.notify_one();
        let handle = lock(&self.0.supervisor).take();
        if let Some(handle) = handle {
            let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
        }
    }

    pub fn state(&self) -> PlayerState {
        lock(&self.0.state).clone()
    }

    pub fn artwork(&self) -> &ArtworkStore {
        &self.0.artwork
    }

    fn library(&self) -> Option<LibraryService> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|library| library.inner().clone())
    }

    pub fn queue(&self) -> QueueView {
        lock(&self.0.queue).view()
    }

    /// Plays files and folders (opened, dropped or passed at launch), replacing the queue.
    pub async fn open(&self, paths: Vec<PathBuf>) -> Result<(), AppError> {
        let files = tauri::async_runtime::spawn_blocking(move || expand(paths)).await?;
        if files.is_empty() {
            return Err(AppError::NotFound("no playable files".into()));
        }
        let entries = self.file_entries(files).await;
        let untagged: Vec<String> = entries
            .iter()
            .filter(|entry| entry.track_id.is_none())
            .map(|entry| entry.path.clone())
            .collect();
        self.play_entries(entries, 0).await?;
        self.read_entry_tags(untagged);
        Ok(())
    }

    /// Queue entries for files: library rows where the library knows them, else just a title
    /// from the file name until [`Self::read_entry_tags`] fills in more.
    async fn file_entries(&self, files: Vec<PathBuf>) -> Vec<Entry> {
        let paths: Vec<String> = files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let known = match self.library() {
            Some(library) => {
                let paths = paths.clone();
                library
                    .run(move |db| {
                        paths
                            .iter()
                            .map(|path| db.track_by_path(path))
                            .collect::<library::Result<Vec<_>>>()
                    })
                    .await
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| match known.get(index).cloned().flatten() {
                Some(track) => entry_from_track(track),
                None => Entry {
                    title: file_title(Path::new(&path)),
                    path,
                    ..Entry::default()
                },
            })
            .collect()
    }

    /// Reads tags of queued files outside the library in the background, so the queue shows
    /// artists and durations.
    fn read_entry_tags(&self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            for chunk in paths.chunks(50) {
                let chunk = chunk.to_vec();
                let Ok(read) = tauri::async_runtime::spawn_blocking(move || {
                    chunk
                        .into_iter()
                        .filter_map(|path| {
                            let info = media::read_track(Path::new(&path)).ok()?;
                            Some((path, info.metadata))
                        })
                        .collect::<Vec<_>>()
                })
                .await
                else {
                    return;
                };
                let mut changed = false;
                {
                    let mut queue = lock(&player.0.queue);
                    for (path, metadata) in read {
                        changed |= queue.update_entries(&path, |entry| {
                            fill_entry(entry, &metadata);
                        });
                    }
                }
                if changed {
                    player.queue_changed_quietly();
                }
            }
        });
    }

    /// Replaces the queue and plays from `start`.
    pub async fn play_entries(&self, entries: Vec<Entry>, start: usize) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let item = {
            let mut queue = lock(&self.0.queue);
            queue.replace(entries, start, seed());
            queue.current().cloned()
        };
        self.emit_queue();
        match item {
            Some(item) => self.load(&item).await,
            None => Ok(()),
        }
    }

    /// Adds entries after the current item or at the end. Adding to an empty queue starts
    /// playing.
    pub async fn enqueue(&self, entries: Vec<Entry>, next: bool) -> Result<(), AppError> {
        if entries.is_empty() {
            return Ok(());
        }
        let _transport = self.0.transport.lock().await;
        let start = {
            let mut queue = lock(&self.0.queue);
            if next {
                queue.play_next(entries);
            } else {
                queue.append(entries);
            }
            if queue.current().is_none() {
                let first = queue.items().first().map(|item| item.uid);
                first.and_then(|uid| queue.jump(uid).cloned())
            } else {
                None
            }
        };
        match start {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => self.queue_changed().await,
        }
    }

    pub async fn jump(&self, uid: u64) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let item = lock(&self.0.queue).jump(uid).cloned();
        match item {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => Ok(()),
        }
    }

    pub async fn next(&self) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let item = lock(&self.0.queue).advance(true).cloned();
        match item {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => Ok(()),
        }
    }

    /// Restarts the track, or goes to the previous one when near its start.
    pub async fn previous(&self) -> Result<(), AppError> {
        let (position, status) = {
            let state = lock(&self.0.state);
            (state.position, state.status)
        };
        if position <= RESTART_THRESHOLD || status == Status::Idle {
            let _transport = self.0.transport.lock().await;
            let item = lock(&self.0.queue).back().cloned();
            if let Some(item) = item {
                self.emit_queue();
                return self.load(&item).await;
            }
        }
        if status == Status::Idle {
            return Ok(());
        }
        self.seek(0.0).await
    }

    /// Applies an edit that doesn't change the playing item.
    pub async fn edit_queue(&self, edit: impl FnOnce(&mut Queue)) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        edit(&mut lock(&self.0.queue));
        self.queue_changed().await
    }

    pub async fn remove(&self, uids: Vec<u64>) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.remove(&uids)).await
    }

    pub async fn move_item(&self, from: usize, to: usize) -> Result<(), AppError> {
        self.edit_queue(|queue| {
            queue.move_item(from, to);
        })
        .await
    }

    pub async fn clear_upcoming(&self) -> Result<(), AppError> {
        self.edit_queue(Queue::clear_upcoming).await
    }

    pub async fn set_shuffle(&self, shuffle: bool) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.set_shuffle(shuffle, seed()))
            .await
    }

    pub async fn set_repeat(&self, repeat: Repeat) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.set_repeat(repeat)).await
    }

    /// Shows a queue item as the current track and reads its tags and artwork.
    fn begin_track(&self, item: &QueueItem) {
        let id = self.0.next_track_id.fetch_add(1, Ordering::Relaxed);
        let entry = &item.entry;
        {
            let mut state = lock(&self.0.state);
            // Tracks from the same folder usually share a cover: keep showing it until the
            // new one is read, instead of flashing an empty backdrop between them.
            let same_folder = state.track.as_ref().is_some_and(|track| {
                Path::new(&track.path).parent() == Path::new(&entry.path).parent()
            });
            let artwork = state
                .track
                .take()
                .and_then(|track| track.artwork)
                .filter(|_| same_folder);
            state.track = Some(Track {
                id,
                path: entry.path.clone(),
                library_id: entry.track_id,
                title: entry.title.clone(),
                metadata: TrackMetadata {
                    artist: entry.artist.clone(),
                    album: entry.album.clone(),
                    duration: entry.duration,
                    ..TrackMetadata::default()
                },
                artwork,
            });
            state.status = Status::Loading;
            state.position = 0.0;
            state.duration = entry.duration;
            state.error = None;
        }
        lock(&self.0.mirror).counted = false;
        self.emit_state();
        self.read_metadata(id, PathBuf::from(&entry.path));
    }

    /// Plays a queue item from the start.
    async fn load(&self, item: &QueueItem) -> Result<(), AppError> {
        self.begin_track(item);
        // Files opened at launch arrive before mpv is up.
        let client = self.wait_for_client().await?;
        command(
            &client,
            vec![json!("set_property"), json!("pause"), json!(false)],
        )
        .await?;
        self.load_with(&client, item).await
    }

    /// Makes `item` mpv's playing file and preloads the one after it.
    async fn load_with(&self, client: &Client, item: &QueueItem) -> Result<(), AppError> {
        *lock(&self.0.mirror) = Mirror::default();
        let reply = command_value(
            client,
            vec![json!("loadfile"), json!(item.entry.path), json!("replace")],
        )
        .await?;
        lock(&self.0.mirror).current = entry_id(&reply).map(|id| (id, item.uid));
        self.sync_next(client, true).await
    }

    /// Mirrors the queue's next item into mpv's playlist (unless it's there already).
    async fn sync_next(&self, client: &Client, force: bool) -> Result<(), AppError> {
        let wanted = lock(&self.0.queue).peek_next().cloned();
        {
            let mirror = lock(&self.0.mirror);
            // Nothing playing, nothing to preload.
            if mirror.current.is_none() {
                return Ok(());
            }
            if !force && mirror.next.map(|(_, uid)| uid) == wanted.as_ref().map(|i| i.uid) {
                return Ok(());
            }
        }
        // Clears everything but the playing file.
        command(client, vec![json!("playlist-clear")]).await?;
        lock(&self.0.mirror).next = None;
        if let Some(item) = wanted {
            let reply = command_value(
                client,
                vec![json!("loadfile"), json!(item.entry.path), json!("append")],
            )
            .await?;
            lock(&self.0.mirror).next = entry_id(&reply).map(|id| (id, item.uid));
        }
        Ok(())
    }

    /// mpv moved on to the preloaded file by itself (gapless): follow it in the queue.
    async fn advanced(&self, client: &Client, entry: i64) {
        let _transport = self.0.transport.lock().await;
        let uid = {
            let mut mirror = lock(&self.0.mirror);
            // A user action may have replaced the playlist meanwhile.
            let Some((id, uid)) = mirror.next.filter(|(id, _)| *id == entry) else {
                return;
            };
            mirror.current = Some((id, uid));
            mirror.next = None;
            uid
        };
        let item = lock(&self.0.queue).jump(uid).cloned();
        if let Some(item) = &item {
            self.begin_track(item);
        }
        self.emit_queue();
        if let Err(err) = self.sync_next(client, true).await {
            log::warn!("cannot preload the next track: {err}");
        }
    }

    async fn queue_changed(&self) -> Result<(), AppError> {
        self.emit_queue();
        match self.client() {
            Ok(client) => self.sync_next(&client, false).await,
            Err(_) => Ok(()),
        }
    }

    /// Display-only queue changes (tags read in the background).
    fn queue_changed_quietly(&self) {
        self.emit_queue();
    }

    /// Counts a play of a library track once enough of it was heard.
    fn count_play(&self) {
        let (position, duration, library_id) = {
            let state = lock(&self.0.state);
            (
                state.position,
                state.duration,
                state.track.as_ref().and_then(|t| t.library_id),
            )
        };
        let Some(track_id) = library_id else {
            return;
        };
        let threshold = duration
            .filter(|d| *d > 0.0)
            .map_or(30.0, |d| (d / 2.0).min(COUNT_AFTER_MAX));
        if position < threshold {
            return;
        }
        {
            let mut mirror = lock(&self.0.mirror);
            if mirror.counted {
                return;
            }
            mirror.counted = true;
        }
        if let Some(library) = self.library() {
            library.record_play(track_id);
        }
    }

    /// A file's tags were edited: refresh its queue entries and, if it is playing, the
    /// current track.
    pub async fn file_retagged(&self, path: PathBuf) {
        let key = path.to_string_lossy().into_owned();
        let entry = match self.library() {
            Some(library) => {
                let key = key.clone();
                library
                    .run(move |db| db.track_by_path(&key))
                    .await
                    .ok()
                    .flatten()
                    .map(entry_from_track)
            }
            None => None,
        };
        let entry = match entry {
            Some(entry) => entry,
            None => {
                let file = path.clone();
                let metadata = tauri::async_runtime::spawn_blocking(move || {
                    media::read_track(&file).map(|info| info.metadata).ok()
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let mut entry = Entry {
                    path: key.clone(),
                    title: file_title(&path),
                    ..Entry::default()
                };
                fill_entry(&mut entry, &metadata);
                entry
            }
        };
        lock(&self.0.queue).update_entries(&key, |queued| {
            queued.title.clone_from(&entry.title);
            queued.artist.clone_from(&entry.artist);
            queued.album.clone_from(&entry.album);
            queued.cover.clone_from(&entry.cover);
        });
        self.emit_queue();

        let current = {
            let mut state = lock(&self.0.state);
            state.track.as_mut().filter(|t| t.path == key).map(|track| {
                track.metadata = TrackMetadata::default();
                track.title.clone_from(&entry.title);
                track.id
            })
        };
        if let Some(id) = current {
            self.emit_state();
            self.read_metadata(id, path);
        }
    }

    pub async fn play(&self) -> Result<(), AppError> {
        let status = lock(&self.0.state).status;
        if status == Status::Idle {
            // The queue ended (or nothing was loaded yet): play the current item again.
            let _transport = self.0.transport.lock().await;
            let item = lock(&self.0.queue).current().cloned();
            return match item {
                Some(item) => self.load(&item).await,
                None => Ok(()),
            };
        }
        self.set_property("pause", json!(false)).await
    }

    pub async fn pause(&self) -> Result<(), AppError> {
        self.set_property("pause", json!(true)).await
    }

    pub async fn toggle(&self) -> Result<(), AppError> {
        let playing = matches!(
            lock(&self.0.state).status,
            Status::Playing | Status::Loading
        );
        if playing {
            self.pause().await
        } else {
            self.play().await
        }
    }

    pub async fn seek(&self, position: f64) -> Result<(), AppError> {
        let position = position.max(0.0);
        let client = self.client()?;
        command(
            &client,
            vec![json!("seek"), json!(position), json!("absolute+exact")],
        )
        .await?;
        lock(&self.0.state).position = position;
        self.emit_position(true);
        Ok(())
    }

    pub async fn set_volume(&self, volume: f64) -> Result<(), AppError> {
        self.set_property("volume", json!(volume.clamp(0.0, 100.0)))
            .await
    }

    pub async fn set_muted(&self, muted: bool) -> Result<(), AppError> {
        self.set_property("mute", json!(muted)).await
    }

    pub async fn set_speed(&self, speed: f64) -> Result<(), AppError> {
        self.set_property("speed", json!(speed.clamp(0.25, 4.0)))
            .await
    }

    async fn set_property(&self, name: &str, value: Value) -> Result<(), AppError> {
        let client = self.client()?;
        command(&client, vec![json!("set_property"), json!(name), value]).await
    }

    fn client(&self) -> Result<Arc<Client>, AppError> {
        self.0
            .client
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| AppError::Player("the playback engine is not running".into()))
    }

    async fn wait_for_client(&self) -> Result<Arc<Client>, AppError> {
        let deadline = tokio::time::Instant::now() + READY_TIMEOUT;
        loop {
            // Register interest before checking, so a connection made in between isn't missed.
            let ready = self.0.ready.notified();
            tokio::pin!(ready);
            ready.as_mut().enable();
            if let Ok(client) = self.client() {
                return Ok(client);
            }
            if lock(&self.0.state).error == Some(PlayerError::MpvMissing)
                || tokio::time::timeout_at(deadline, ready).await.is_err()
            {
                return self.client();
            }
        }
    }

    fn set_client(&self, client: Option<Arc<Client>>) {
        let connected = client.is_some();
        *self
            .0
            .client
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = client;
        if connected {
            self.0.ready.notify_waiters();
        }
    }

    /// Reads tags and artwork off the async runtime, then fills them into the track if it is
    /// still the current one.
    fn read_metadata(&self, id: u64, path: PathBuf) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = tauri::async_runtime::spawn_blocking(move || {
                let info = media::read_track(&path)
                    .inspect_err(|err| log::info!("tags of {}: {err}", path.display()))
                    .unwrap_or_default();
                let artwork = info
                    .embedded_artwork
                    .or_else(|| media::folder_artwork(&path))
                    .and_then(|art| {
                        media::prepare_artwork(art)
                            .inspect_err(|err| log::info!("artwork of {}: {err}", path.display()))
                            .ok()
                    });
                (info.metadata, artwork)
            })
            .await;
            let Ok((metadata, artwork)) = result else {
                return;
            };

            let artwork = artwork.map(|prepared| {
                let palette = prepared.palette.clone();
                ArtworkInfo {
                    id: player.0.artwork.insert(prepared),
                    palette,
                }
            });
            let track = {
                let mut state = lock(&player.0.state);
                let Some(track) = state.track.as_mut().filter(|t| t.id == id) else {
                    return;
                };
                fill_missing(&mut track.metadata, metadata);
                if let Some(title) = &track.metadata.title {
                    track.title.clone_from(title);
                }
                track.artwork = artwork;
                track.clone()
            };
            let queued = lock(&player.0.queue).update_entries(&track.path, |entry| {
                if entry.track_id.is_none() {
                    fill_entry(entry, &track.metadata);
                }
            });
            if queued {
                player.queue_changed_quietly();
            }
            player.emit_state();
            player.emit(TRACK_EVENT, &track);
        });
    }

    /// Formats lofty can't read (DSF, WMA, ...) still have tags mpv/FFmpeg understands.
    async fn fill_from_mpv_metadata(&self, client: &Client) {
        let needs_tags = lock(&self.0.state)
            .track
            .as_ref()
            .is_some_and(|t| t.metadata.title.is_none());
        if !needs_tags {
            return;
        }
        let Ok(Ok(tags)) = tokio::time::timeout(
            COMMAND_TIMEOUT,
            client.get_property::<serde_json::Map<String, Value>>("metadata"),
        )
        .await
        else {
            return;
        };
        let tag = |key: &str| {
            tags.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .and_then(|(_, v)| v.as_str())
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let from_mpv = TrackMetadata {
            title: tag("title"),
            artist: tag("artist"),
            album: tag("album"),
            album_artist: tag("album_artist"),
            genre: tag("genre"),
            ..TrackMetadata::default()
        };
        {
            let mut state = lock(&self.0.state);
            let Some(track) = state.track.as_mut() else {
                return;
            };
            fill_missing(&mut track.metadata, from_mpv);
            if let Some(title) = &track.metadata.title {
                track.title.clone_from(title);
            }
        }
        self.emit_state();
    }

    async fn handle_event(&self, event: Event, client: &Client) {
        let mut apply = true;
        let mut stop = false;
        match &event {
            Event::FileLoaded => {
                lock(&self.0.mirror).failures = 0;
                let resume = lock(&self.0.pending_seek).take();
                if let Some(position) = resume {
                    let _ = command(
                        client,
                        vec![json!("seek"), json!(position), json!("absolute+exact")],
                    )
                    .await;
                }
                self.fill_from_mpv_metadata(client).await;
            }
            Event::StartFile {
                playlist_entry_id: Some(entry),
            } => {
                let preloaded = lock(&self.0.mirror).next.map(|(id, _)| id);
                if preloaded == Some(*entry) {
                    self.advanced(client, *entry).await;
                }
            }
            Event::EndFile {
                reason,
                playlist_entry_id,
                ..
            } => {
                let mut mirror = lock(&self.0.mirror);
                let is_current = mirror
                    .current
                    .is_some_and(|(id, _)| Some(id) == *playlist_entry_id);
                if is_current {
                    let has_next = mirror.next.is_some();
                    match reason {
                        // mpv goes straight on to the preloaded file: no idle in between.
                        EndReason::Eof if has_next => apply = false,
                        EndReason::Error => {
                            mirror.failures += 1;
                            if mirror.failures >= MAX_SKIPS {
                                stop = has_next;
                            } else if has_next {
                                // Skip unplayable files quietly, like other players do.
                                log::info!("skipping an unplayable file");
                                apply = false;
                            }
                        }
                        _ => {}
                    }
                    if apply && matches!(reason, EndReason::Eof | EndReason::Error) {
                        mirror.current = None;
                    }
                }
            }
            _ => {}
        }
        if stop {
            log::warn!("{MAX_SKIPS} files in a row failed to play; stopping");
            *lock(&self.0.mirror) = Mirror::default();
            let _ = command(client, vec![json!("stop")]).await;
        }
        if !apply {
            return;
        }
        let changes = lock(&self.0.state).apply(&event);
        if changes.state {
            self.emit_state();
        } else if changes.position {
            self.emit_position(false);
        }
        if changes.position {
            self.count_play();
        }
    }

    /// Sets up a freshly started mpv: observers, volume/speed, and the track it was playing.
    async fn attach(&self, client: &Client) -> Result<(), AppError> {
        let state = self.state();
        command(
            client,
            vec![json!("set_property"), json!("volume"), json!(state.volume)],
        )
        .await?;
        command(
            client,
            vec![json!("set_property"), json!("mute"), json!(state.muted)],
        )
        .await?;
        command(
            client,
            vec![json!("set_property"), json!("speed"), json!(state.speed)],
        )
        .await?;
        command(
            client,
            vec![json!("set_property"), json!("pause"), json!(state.paused)],
        )
        .await?;
        for (id, name) in OBSERVED.iter().enumerate() {
            client
                .observe_property(id as u64 + 1, name)
                .await
                .map_err(mpv_error)?;
        }
        let current = lock(&self.0.queue).current().cloned();
        if let (Status::Playing | Status::Paused | Status::Loading, Some(item)) =
            (state.status, current)
        {
            *lock(&self.0.pending_seek) = Some(state.position).filter(|p| *p > 0.0);
            self.load_with(client, &item).await?;
        }
        Ok(())
    }

    fn fail(&self, error: PlayerError) {
        {
            let mut state = lock(&self.0.state);
            state.error = Some(error);
            state.status = Status::Idle;
        }
        self.emit_state();
    }

    fn emit_state(&self) {
        let state = self.state();
        *lock(&self.0.last_position_emit) = Instant::now();
        self.emit(STATE_EVENT, &state);
    }

    fn emit_position(&self, force: bool) {
        {
            let mut last = lock(&self.0.last_position_emit);
            if !force && last.elapsed() < POSITION_INTERVAL {
                return;
            }
            *last = Instant::now();
        }
        let update = {
            let state = lock(&self.0.state);
            PositionUpdate {
                position: state.position,
                duration: state.duration,
            }
        };
        self.emit(POSITION_EVENT, &update);
    }

    fn emit_queue(&self) {
        self.0.queue_dirty.store(true, Ordering::Relaxed);
        let view = self.queue();
        self.emit(QUEUE_EVENT, &view);
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: &S) {
        if let Err(err) = self.0.app.emit(event, payload) {
            log::warn!("cannot emit {event}: {err}");
        }
    }
}

async fn command(client: &Client, args: Vec<Value>) -> Result<(), AppError> {
    command_value(client, args).await.map(drop)
}

async fn command_value(client: &Client, args: Vec<Value>) -> Result<Value, AppError> {
    match tokio::time::timeout(COMMAND_TIMEOUT, client.command(args)).await {
        Ok(result) => result.map_err(mpv_error),
        Err(_) => Err(AppError::Player("mpv did not respond".into())),
    }
}

/// The playlist entry id `loadfile` reports (mpv 0.33+).
fn entry_id(reply: &Value) -> Option<i64> {
    let id = reply.get("playlist_entry_id").and_then(Value::as_i64);
    if id.is_none() {
        log::warn!("mpv did not report a playlist entry id; gapless playback is off");
    }
    id
}

/// A shuffle seed; any changing value will do.
fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x5EED, |d| d.as_nanos() as u64)
}

/// Fills an entry's display fields from tags.
fn fill_entry(entry: &mut Entry, metadata: &TrackMetadata) {
    if let Some(title) = &metadata.title {
        entry.title.clone_from(title);
    }
    entry.artist = metadata.artist.clone().or(entry.artist.take());
    entry.album = metadata.album.clone().or(entry.album.take());
    entry.duration = metadata.duration.or(entry.duration);
}

/// Files to play for opened paths: files as they are, folders as the audio files inside
/// them (recursively, sorted by path).
fn expand(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            let mut stack = vec![path];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let hidden = entry.file_name().to_string_lossy().starts_with('.');
                    if hidden {
                        continue;
                    }
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(path);
                    } else if library::is_audio_file(&path) {
                        found.push(path);
                    }
                }
            }
            found.sort();
            files.extend(found);
        } else if path.is_file() {
            files.push(path);
        }
        if files.len() >= MAX_OPEN {
            files.truncate(MAX_OPEN);
            break;
        }
    }
    files
}

fn mpv_error(err: mpv_ipc::Error) -> AppError {
    AppError::Player(err.to_string())
}

/// The file name without extension, shown until (or unless) tags provide a title.
fn file_title(path: &Path) -> String {
    path.file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

/// Copies every field `target` doesn't have yet from `source`.
fn fill_missing(target: &mut TrackMetadata, source: TrackMetadata) {
    macro_rules! fill {
        ($($field:ident),*) => { $( if target.$field.is_none() { target.$field = source.$field; } )* };
    }
    fill!(
        title,
        artist,
        album,
        album_artist,
        genre,
        year,
        track_number,
        disc_number,
        duration,
        format,
        sample_rate,
        bit_depth,
        bitrate,
        channels
    );
}

/// Command-line flags for mpv. `--no-config` keeps the user's mpv.conf (and the bundle's
/// pseudo-GUI config) out; media keys and Now Playing belong to Vitra, not mpv.
fn mpv_args(with_newer_options: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--idle=yes",
        "--no-config",
        "--load-scripts=no",
        "--no-terminal",
        "--no-video",
        "--audio-display=no",
        "--input-default-bindings=no",
        "--input-media-keys=no",
        "--audio-client-name=Vitra",
        "--ytdl-format=bestaudio/best",
        // Opens the preloaded next file early, so gapless transitions don't wait on disk
        // or network.
        "--prefetch-playlist=yes",
    ]
    .map(OsString::from)
    .to_vec();
    // Only in recent mpv (0.39+); older builds refuse to start with them, see `supervise`.
    if with_newer_options {
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            args.push("--media-controls=no".into());
        }
        if cfg!(target_os = "macos") {
            // No Dock icon for the helper process.
            args.push("--macos-app-activation-policy=prohibited".into());
        }
    }
    // Debugging aid, e.g. VITRA_MPV_ARGS="--ao=null --log-file=/tmp/mpv.log".
    if let Ok(extra) = std::env::var("VITRA_MPV_ARGS") {
        args.extend(extra.split_whitespace().map(OsString::from));
    }
    args
}

async fn supervise(player: Player) {
    let paths = sidecar::search_paths(&player.0.app);
    let Some(mpv) = paths.locate(Sidecar::Mpv) else {
        log::error!("mpv not found");
        player.fail(PlayerError::MpvMissing);
        return;
    };
    log::info!("using mpv at {} ({:?})", mpv.path.display(), mpv.source);

    let mut newer_options = true;
    let mut failures: VecDeque<Instant> = VecDeque::new();
    loop {
        let spawned = MpvProcess::spawn(SpawnOptions {
            binary: mpv.path.clone(),
            args: mpv_args(newer_options),
            pipe_prefix: "vitra-mpv".into(),
            startup_timeout: STARTUP_TIMEOUT,
        })
        .await;

        let error = match spawned {
            Ok((mut process, client, mut events)) => {
                log::info!("{} started", process.version());
                let client = Arc::new(client);
                if let Err(err) = player.attach(&client).await {
                    log::warn!("setting up mpv failed: {err}");
                }
                player.set_client(Some(Arc::clone(&client)));
                {
                    let mut state = lock(&player.0.state);
                    if matches!(state.error, Some(PlayerError::MpvFailed { .. })) {
                        state.error = None;
                    }
                }
                player.emit_state();

                let reason = loop {
                    tokio::select! {
                        event = events.recv() => match event {
                            Some(event) => player.handle_event(event, &client).await,
                            None => break "lost the IPC connection".to_owned(),
                        },
                        status = process.wait() => break format!("exited ({status:?})"),
                        () = player.0.shutdown.notified() => {
                            player.set_client(None);
                            process.shutdown(&client, Duration::from_secs(2)).await;
                            return;
                        }
                    }
                };
                player.set_client(None);
                reason
            }
            Err(mpv_ipc::Error::Exited(status)) if newer_options => {
                log::warn!("mpv exited at startup ({status}); retrying without newer options");
                newer_options = false;
                continue;
            }
            Err(err) => err.to_string(),
        };

        log::warn!("mpv stopped: {error}");
        let now = Instant::now();
        failures.push_back(now);
        while failures
            .front()
            .is_some_and(|t| now.duration_since(*t) > RESTART_WINDOW)
        {
            failures.pop_front();
        }
        if failures.len() >= MAX_FAILURES {
            player.fail(PlayerError::MpvFailed { message: error });
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(failures.len() as u64)) => {}
            () = player.0.shutdown.notified() => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_fall_back_to_the_file_name() {
        assert_eq!(
            file_title(Path::new("/music/01 - Intro.flac")),
            "01 - Intro"
        );
        assert_eq!(file_title(Path::new("noext")), "noext");
    }

    #[test]
    fn fill_missing_keeps_existing_values() {
        let mut target = TrackMetadata {
            title: Some("From tags".into()),
            ..TrackMetadata::default()
        };
        fill_missing(
            &mut target,
            TrackMetadata {
                title: Some("From mpv".into()),
                artist: Some("Artist".into()),
                ..TrackMetadata::default()
            },
        );
        assert_eq!(target.title.as_deref(), Some("From tags"));
        assert_eq!(target.artist.as_deref(), Some("Artist"));
    }

    #[test]
    fn opening_a_folder_queues_its_audio_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let album = dir.path().join("Album");
        std::fs::create_dir_all(album.join("CD2")).unwrap();
        std::fs::create_dir_all(album.join(".hidden")).unwrap();
        for name in [
            "02.flac",
            "01.flac",
            "cover.jpg",
            "CD2/01.mp3",
            ".hidden/x.mp3",
        ] {
            std::fs::write(album.join(name), b"").unwrap();
        }
        let loose = dir.path().join("loose.ogg");
        std::fs::write(&loose, b"").unwrap();

        let files = expand(vec![
            loose.clone(),
            album.clone(),
            dir.path().join("gone.mp3"),
        ]);
        assert_eq!(
            files,
            [
                loose,
                album.join("01.flac"),
                album.join("02.flac"),
                album.join("CD2/01.mp3"),
            ]
        );
    }

    #[test]
    fn entries_take_tags_but_keep_what_tags_lack() {
        let mut entry = Entry {
            path: "/a.mp3".into(),
            title: "a".into(),
            album: Some("Known".into()),
            ..Entry::default()
        };
        fill_entry(
            &mut entry,
            &TrackMetadata {
                title: Some("Song".into()),
                artist: Some("Artist".into()),
                duration: Some(10.0),
                ..TrackMetadata::default()
            },
        );
        assert_eq!(entry.title, "Song");
        assert_eq!(entry.artist.as_deref(), Some("Artist"));
        assert_eq!(entry.album.as_deref(), Some("Known"));
        assert_eq!(entry.duration, Some(10.0));
    }

    #[test]
    fn version_specific_flags_can_be_dropped() {
        let newer = mpv_args(true);
        let older = mpv_args(false);
        assert!(older.iter().all(|a| a != "--media-controls=no"));
        assert!(newer.len() >= older.len());
        for required in [
            "--no-config",
            "--idle=yes",
            "--no-video",
            "--input-media-keys=no",
        ] {
            assert!(older.iter().any(|a| a == required), "{required}");
        }
    }
}
