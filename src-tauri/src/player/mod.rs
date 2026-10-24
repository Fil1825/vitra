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
mod level;
mod queue;
mod session;
mod spectrum;
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
use crate::links::LinkService;
use crate::lyrics::{LyricsService, TrackInfo};
use crate::now_playing::NowPlaying;
use crate::settings::{AudioMeters, SettingsStore};
use crate::sidecar::{self, Sidecar};

pub use artwork::{
    ArtworkStore, COVER_PREFIX, SCHEME as ARTWORK_SCHEME, respond_cover, status as http_status,
};
pub use commands::*;
pub use queue::{Entry, QueueView, Repeat};
use queue::{Queue, QueueItem};
use state::{ArtworkInfo, OBSERVED, PlayerError, Track};
pub use state::{PlayerState, Status};

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
    /// The playing entry was a playlist (a radio station's `.m3u`/`.pls`) that mpv replaced
    /// with its entries: the next `start-file` is the same queue item.
    redirected: bool,
    /// Playback stopped on an error: mpv may already be starting the preloaded file, which
    /// must not replace the error with a loading state. Cleared by the next load.
    halted: bool,
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
    /// A load is waiting for mpv to come up; a freshly started mpv leaves loading to it.
    load_waiting: AtomicBool,
    /// The queue changed since the session was last saved.
    queue_dirty: AtomicBool,
    /// The playback values last saved (JSON), to skip writing unchanged ones.
    saved_playback: Mutex<String>,
    /// What mpv's yt-dlp hook logged while opening the current link (see [`hook_line`]).
    hook_log: Mutex<Vec<String>>,
    /// A window shows the backdrop's pulse (see [`level`]) or the spectrum (see
    /// [`spectrum`]): read them from mpv while one does.
    pulse_wanted: AtomicBool,
    spectrum_wanted: AtomicBool,
    meters_running: AtomicBool,
}

/// Lines of the hook's log kept per link: its errors are a few lines of yt-dlp's stderr.
const HOOK_LOG_LINES: usize = 40;

/// The text of a `log-message` event from mpv's yt-dlp hook (warnings and errors).
fn hook_line(raw: &Value) -> Option<String> {
    let field = |key: &str| raw.get(key).and_then(Value::as_str);
    let level = field("level")?;
    (field("prefix")? == "ytdl_hook" && matches!(level, "error" | "warn" | "fatal"))
        .then(|| field("text").map(|text| text.trim_end().to_owned()))
        .flatten()
}

/// Why a link failed to open, from its hook's log, when the user can do something about it
/// (cookies, an age limit, a sign-in): then playback stops with that instead of skipping on,
/// since the next links would fail the same way.
fn link_failure(log: &[String]) -> Option<links::LinkError> {
    if log.is_empty() {
        return None;
    }
    match links::ytdlp::failure(&log.join("\n")) {
        links::LinkError::Failed(_) => None,
        known => Some(known),
    }
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
            load_waiting: AtomicBool::new(false),
            queue_dirty: AtomicBool::new(false),
            saved_playback: Mutex::new(String::new()),
            hook_log: Mutex::new(Vec::new()),
            pulse_wanted: AtomicBool::new(false),
            spectrum_wanted: AtomicBool::new(false),
            meters_running: AtomicBool::new(false),
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

    fn lyrics(&self) -> Option<LyricsService> {
        self.0
            .app
            .try_state::<LyricsService>()
            .map(|lyrics| lyrics.inner().clone())
    }

    /// Tells the lyrics where playback is.
    fn follow_lyrics(&self) {
        let playing = {
            let state = lock(&self.0.state);
            state.track.as_ref().map(|track| (track.id, state.position))
        };
        if let (Some((id, position)), Some(lyrics)) = (playing, self.lyrics()) {
            lyrics.position(id, position);
        }
    }

    /// The playing track joined or left the library.
    fn library_id_changed(&self) {
        let track = lock(&self.0.state)
            .track
            .as_ref()
            .map(|track| (track.id, track.library_id));
        if let (Some((id, library_id)), Some(lyrics)) = (track, self.lyrics()) {
            lyrics.library_id_changed(id, library_id);
        }
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
        let entries = self.file_entries(files.clone()).await;
        let untagged: Vec<String> = entries
            .iter()
            .filter(|entry| entry.track_id.is_none())
            .map(|entry| entry.path.clone())
            .collect();
        self.play_entries(entries, 0).await?;
        let library = self.library().filter(|_| self.adds_opened_files());
        match library {
            // Playback starts right away; the files join the library in the background.
            Some(library) if !untagged.is_empty() => self.import(library, files),
            _ => self.read_entry_tags(untagged),
        }
        Ok(())
    }

    fn adds_opened_files(&self) -> bool {
        self.0
            .app
            .try_state::<SettingsStore>()
            .is_some_and(|settings| settings.get().library.add_opened_files)
    }

    /// Adds opened files to the library, then links their queue entries to the new tracks.
    fn import(&self, library: LibraryService, files: Vec<PathBuf>) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            match tauri::async_runtime::spawn_blocking(move || library.add_files(&files)).await {
                Ok(Ok(tracks)) => player.link_tracks(&tracks),
                Ok(Err(err)) => log::warn!("cannot add opened files to the library: {err}"),
                Err(err) => log::warn!("adding opened files failed: {err}"),
            }
        });
    }

    /// Queued files became library tracks: give their entries (and the playing track) the
    /// track ids, so favorites, history and playlists work for them.
    fn link_tracks(&self, tracks: &[library::Track]) {
        let mut queued = false;
        {
            let mut queue = lock(&self.0.queue);
            for track in tracks {
                let entry = entry_from_track(track.clone());
                queued |= queue.update_entries(&track.path, |queued| *queued = entry.clone());
            }
        }
        let playing = {
            let mut state = lock(&self.0.state);
            match state.track.as_mut() {
                Some(current) => tracks
                    .iter()
                    .find(|track| track.path == current.path)
                    .is_some_and(|track| {
                        let linked = current.library_id != Some(track.id);
                        current.library_id = Some(track.id);
                        linked
                    }),
                None => false,
            }
        };
        if queued {
            self.emit_queue();
        }
        if playing {
            self.emit_state();
            self.library_id_changed();
        }
    }

    /// Tracks left the library: their queue entries become plain files again.
    pub fn tracks_removed(&self, ids: &[i64]) {
        let queued = lock(&self.0.queue).update_where(
            |entry| entry.track_id.is_some_and(|id| ids.contains(&id)),
            |entry| {
                entry.track_id = None;
                entry.cover = None;
            },
        );
        let playing = {
            let mut state = lock(&self.0.state);
            match state.track.as_mut() {
                Some(track) if track.library_id.is_some_and(|id| ids.contains(&id)) => {
                    track.library_id = None;
                    true
                }
                _ => false,
            }
        };
        if queued {
            self.emit_queue();
        }
        if playing {
            self.emit_state();
            self.library_id_changed();
        }
    }

    /// Library tracks got more complete metadata (a playlist of links being filled in) or a
    /// cover (a link album's, one the user chose): refresh their queue entries and, if one
    /// is playing, the current track.
    pub fn library_tracks_updated(&self, tracks: &[library::Track]) {
        let queued = {
            let mut queue = lock(&self.0.queue);
            let mut changed = false;
            for track in tracks {
                let entry = entry_from_track(track.clone());
                changed |= queue.update_entries(&track.path, |queued| *queued = entry.clone());
            }
            changed
        };
        let current = {
            let mut state = lock(&self.0.state);
            let duration_known = state.duration.is_some();
            state.track.as_mut().and_then(|current| {
                let track = tracks.iter().find(|track| track.path == current.path)?;
                current.library_id = Some(track.id);
                current.title.clone_from(&track.title);
                current.metadata.title = Some(track.title.clone());
                current.metadata.artist.clone_from(&track.artist);
                current.metadata.album.clone_from(&track.album);
                if !duration_known {
                    current.metadata.duration = track.duration;
                }
                Some((current.id, current.path.clone()))
            })
        };
        self.emit_queue_if(queued);
        if let Some((id, path)) = current {
            self.emit_state();
            self.read_metadata(id, PathBuf::from(path));
        }
    }

    /// A link was downloaded: its queue entries play the file from now on. The playing
    /// stream (if it is one of them) goes on undisturbed.
    pub fn link_downloaded(&self, url: &str, track: &library::Track) {
        let entry = entry_from_track(track.clone());
        let queued = lock(&self.0.queue).update_entries(url, |queued| *queued = entry.clone());
        self.emit_queue_if(queued);
    }

    fn emit_queue_if(&self, changed: bool) {
        if changed {
            self.emit_queue();
        }
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
            let same_folder = !links::is_remote(&entry.path)
                && state.track.as_ref().is_some_and(|track| {
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
                    // A link's entry is all there is to know (the stream's own tags, which
                    // mpv would fill in, are often the uploader's file names).
                    title: links::is_remote(&entry.path).then(|| entry.title.clone()),
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
        // A seek asked for while the previous track was loading doesn't apply to this one.
        *lock(&self.0.pending_seek) = None;
        self.emit_state();
        if let Some(lyrics) = self.lyrics() {
            lyrics.track_started(id, &entry.path, entry.track_id);
        }
        self.read_metadata(id, PathBuf::from(&entry.path));
    }

    /// Plays a queue item from the start.
    async fn load(&self, item: &QueueItem) -> Result<(), AppError> {
        self.begin_track(item);
        // Files opened at launch arrive before mpv is up.
        self.0.load_waiting.store(true, Ordering::Relaxed);
        let client = self.wait_for_client().await;
        self.0.load_waiting.store(false, Ordering::Relaxed);
        let client = client?;
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
        // mpv can't seek a file it is still opening (a link waits for yt-dlp): seek once it
        // is loaded.
        let loading = {
            let mut state = lock(&self.0.state);
            let loading = state.status == Status::Loading && state.track.is_some();
            if loading {
                state.position = position;
            }
            loading
        };
        if loading {
            *lock(&self.0.pending_seek) = Some(position);
            self.emit_position(true);
            return Ok(());
        }
        let client = self.client()?;
        command(
            &client,
            vec![json!("seek"), json!(position), json!("absolute+exact")],
        )
        .await?;
        lock(&self.0.state).position = position;
        self.emit_position(true);
        self.follow_lyrics();
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
            // The cover the user chose for the album comes before the file's artwork.
            let custom = player.custom_cover(&path.to_string_lossy()).await;
            let link = path.to_str().filter(|path| links::is_remote(path));
            let thumbnail = match link {
                Some(url) if custom.is_none() => player.link_thumbnail(url).await,
                _ => None,
            };
            let remote = link.is_some();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let custom = custom.map(|bytes| media::Artwork {
                    bytes,
                    mime: Some("image/jpeg".into()),
                });
                // A link has no file to read: its tags come with its queue entry, its
                // artwork is its thumbnail.
                if remote {
                    let artwork = custom
                        .or_else(|| thumbnail.map(|bytes| media::Artwork { bytes, mime: None }))
                        .and_then(|art| {
                            media::prepare_artwork(art)
                                .inspect_err(|err| log::info!("thumbnail of a link: {err}"))
                                .ok()
                        });
                    return (TrackMetadata::default(), artwork);
                }
                let info = media::read_track(&path)
                    .inspect_err(|err| log::info!("tags of {}: {err}", path.display()))
                    .unwrap_or_default();
                let artwork = custom
                    .or(info.embedded_artwork)
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
            if let Some(lyrics) = player.lyrics() {
                lyrics.track_ready(TrackInfo {
                    id: track.id,
                    path: track.path,
                    library_id: track.library_id,
                    title: track.title,
                    metadata: track.metadata,
                });
            }
        });
    }

    /// The cover the user chose for the album of the track at `path`.
    async fn custom_cover(&self, path: &str) -> Option<Vec<u8>> {
        let library = self.library()?;
        let key = path.to_owned();
        let covers = library.covers_dir().to_path_buf();
        library
            .run(move |db| {
                let cover = db.custom_album_cover(&key)?;
                Ok(cover.and_then(|id| std::fs::read(covers.join(format!("{id}.jpg"))).ok()))
            })
            .await
            .ok()
            .flatten()
    }

    /// A link's artwork: its album's cover if the user put it in an album, else its thumbnail
    /// from a recent resolution (full size), else the library's cached cover.
    async fn link_thumbnail(&self, url: &str) -> Option<Vec<u8>> {
        let recent = self
            .0
            .app
            .try_state::<LinkService>()
            .and_then(|links| links.thumbnail(url));
        let stored = match self.library() {
            Some(library) => {
                let url = url.to_owned();
                let covers = library.covers_dir().to_path_buf();
                let wanted = recent.is_none();
                library
                    .run(move |db| {
                        let Some(track) = db.track_by_path(&url)? else {
                            return Ok(None);
                        };
                        // The library gives a corrected link in an album its album's cover.
                        let albums = track.album_id.is_some() && db.link_edited(&url)?;
                        if !albums && !wanted {
                            return Ok(None);
                        }
                        let read = |id: String| std::fs::read(covers.join(format!("{id}.jpg")));
                        Ok(track
                            .cover
                            .and_then(|id| read(id).ok())
                            .map(|bytes| (bytes, albums)))
                    })
                    .await
                    .ok()
                    .flatten()
            }
            None => None,
        };
        match (stored, recent) {
            (Some((bytes, true)), _) => Some(bytes),
            (_, Some(recent)) => Some(recent.as_ref().clone()),
            (stored, None) => stored.map(|(bytes, _)| bytes),
        }
    }

    /// Internet radio names the song playing in its stream metadata (`icy-title`, usually
    /// "Artist - Title"): show it in place of the station.
    fn stream_title_changed(&self, data: Option<&Value>) {
        let Some(tags) = data.and_then(Value::as_object) else {
            return;
        };
        let tag = |key: &str| {
            tags.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .and_then(|(_, v)| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let Some(song) = tag("icy-title") else {
            return;
        };
        let station = tag("icy-name");
        let track = {
            let mut state = lock(&self.0.state);
            let Some(track) = state
                .track
                .as_mut()
                .filter(|track| links::is_remote(&track.path))
            else {
                return;
            };
            let (artist, title) = match song.split_once(" - ") {
                Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
                    (Some(artist.trim().to_owned()), title.trim().to_owned())
                }
                _ => (station.clone(), song),
            };
            if track.title == title && track.metadata.artist == artist {
                return;
            }
            track.metadata.title = Some(title.clone());
            track.title = title;
            track.metadata.artist = artist;
            track.metadata.album = station.or(track.metadata.album.take());
            track.clone()
        };
        self.emit_state();
        self.emit(TRACK_EVENT, &track);
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
        let mut link_error = None;
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
            Event::Other { name, raw } if name == "log-message" => {
                if let Some(line) = hook_line(raw) {
                    let mut log = lock(&self.0.hook_log);
                    if log.len() < HOOK_LOG_LINES {
                        log.push(line);
                    }
                }
                return;
            }
            Event::StartFile { .. } if lock(&self.0.mirror).halted => return,
            Event::StartFile {
                playlist_entry_id: Some(entry),
            } => {
                lock(&self.0.hook_log).clear();
                let preloaded = {
                    let mut mirror = lock(&self.0.mirror);
                    // The first entry of an expanded playlist stands in for it.
                    if mirror.redirected {
                        mirror.redirected = false;
                        mirror.current = mirror.current.map(|(_, uid)| (*entry, uid));
                    }
                    mirror.next.map(|(id, _)| id)
                };
                if preloaded == Some(*entry) {
                    self.advanced(client, *entry).await;
                }
            }
            Event::PropertyChange { name, data, .. } if name == "metadata" => {
                self.stream_title_changed(data.as_ref());
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
                        EndReason::Redirect => mirror.redirected = true,
                        // mpv goes straight on to the preloaded file: no idle in between.
                        EndReason::Eof if has_next => apply = false,
                        EndReason::Error => {
                            mirror.failures += 1;
                            link_error = link_failure(&lock(&self.0.hook_log));
                            // A link the user has to do something about stops the queue
                            // too: the next links would fail the same way.
                            if link_error.is_some() || mirror.failures >= MAX_SKIPS {
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
            match &link_error {
                Some(err) => log::warn!("a link failed ({err}); stopping"),
                None => log::warn!("{MAX_SKIPS} files in a row failed to play; stopping"),
            }
            *lock(&self.0.mirror) = Mirror {
                halted: true,
                ..Mirror::default()
            };
            let _ = command(client, vec![json!("stop")]).await;
        }
        if !apply {
            return;
        }
        let changes = {
            let mut state = lock(&self.0.state);
            let mut changes = state.apply(&event);
            // In place of mpv's "unrecognized file format": the windows translate it.
            if let Some(err) = link_error {
                state.error = Some(PlayerError::Playback {
                    message: err.to_string(),
                });
                changes.state = true;
            }
            changes
        };
        if changes.state {
            self.emit_state();
        } else if changes.position {
            self.emit_position(false);
        }
        if changes.position {
            self.count_play();
            self.follow_lyrics();
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
        // The yt-dlp hook says why a link failed only in its log; `end-file` just has
        // "unrecognized file format".
        command(client, vec![json!("request_log_messages"), json!("warn")]).await?;
        self.set_ytdl_options(client).await;
        self.set_meter_filters(client, false).await;
        let current = lock(&self.0.queue).current().cloned();
        let waiting = self.0.load_waiting.load(Ordering::Relaxed);
        if let (Status::Playing | Status::Paused | Status::Loading, Some(item), false) =
            (state.status, current, waiting)
        {
            *lock(&self.0.pending_seek) = Some(state.position).filter(|p| *p > 0.0);
            self.load_with(client, &item).await?;
        }
        Ok(())
    }

    /// What mpv's yt-dlp hook passes on to yt-dlp: our JavaScript runtime and, when the user
    /// chose one, the browser to take cookies from (age-restricted videos). The hook reads
    /// them for every link it opens, so a change applies to the next one.
    async fn set_ytdl_options(&self, client: &Client) {
        let paths = sidecar::search_paths(&self.0.app);
        let mut options = serde_json::Map::new();
        if let Some(deno) = paths.locate(Sidecar::Deno) {
            options.insert(
                "js-runtimes".into(),
                json!(format!("deno:{}", deno.path.display())),
            );
        }
        let browser = self
            .0
            .app
            .try_state::<SettingsStore>()
            .and_then(|settings| settings.get().downloads.cookies_browser());
        if let Some(browser) = browser {
            options.insert("cookies-from-browser".into(), json!(browser));
        }
        let set = command(
            client,
            vec![
                json!("set_property"),
                json!("ytdl-raw-options"),
                Value::Object(options),
            ],
        )
        .await;
        if let Err(err) = set {
            log::warn!("cannot pass options to mpv's yt-dlp hook: {err}");
        }
    }

    /// What the settings ask mpv's audio chain to measure: the loudness while the backdrop's
    /// pulse is on, the spectrum while the level meter is in the window's layout. Changing
    /// the chain can click, so it isn't touched for a window being hidden; reading stops
    /// then instead ([`Self::set_meters`]).
    fn audio_meters(&self) -> AudioMeters {
        self.0.app.try_state::<SettingsStore>().map_or(
            AudioMeters {
                pulse: false,
                spectrum: false,
            },
            |settings| settings.get().audio_meters(),
        )
    }

    /// Puts the measuring filters the settings ask for into mpv's audio chain; `replace` takes
    /// out the ones already there first (a fresh mpv has none).
    async fn set_meter_filters(&self, client: &Client, replace: bool) {
        if replace {
            for label in [level::FILTER_LABEL, spectrum::FILTER_LABEL] {
                let label = format!("@{label}");
                let _ = command(client, vec![json!("af"), json!("remove"), json!(label)]).await;
            }
        }
        let meters = self.audio_meters();
        let filters = [
            (meters.pulse, level::filter()),
            (meters.spectrum, spectrum::filter()),
        ];
        for (wanted, filter) in filters {
            if wanted
                && let Err(err) =
                    command(client, vec![json!("af"), json!("add"), json!(filter)]).await
            {
                log::warn!("cannot measure the audio: {err}");
            }
        }
    }

    /// The backdrop pulse was switched, or the level meter shown or hidden.
    pub fn meter_filters_changed(&self) {
        let Ok(client) = self.client() else {
            return;
        };
        let player = self.clone();
        tauri::async_runtime::spawn(async move { player.set_meter_filters(&client, true).await });
    }

    /// What a window shows: the pulse (it is visible, motion is on and so is the setting)
    /// and the spectrum (the level meter is on screen). While one of them is wanted, they are
    /// read and sent as `player://level` and `player://spectrum`.
    pub fn set_meters(&self, pulse: bool, spectrum: bool) {
        self.0.pulse_wanted.store(pulse, Ordering::Relaxed);
        self.0.spectrum_wanted.store(spectrum, Ordering::Relaxed);
        if (pulse || spectrum) && !self.0.meters_running.swap(true, Ordering::Relaxed) {
            tauri::async_runtime::spawn(read_meters(self.clone()));
        }
    }

    /// The link settings (cookies) changed.
    pub fn links_settings_changed(&self) {
        let Ok(client) = self.client() else {
            return;
        };
        let player = self.clone();
        tauri::async_runtime::spawn(async move { player.set_ytdl_options(&client).await });
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
        self.show_now_playing(&state);
    }

    /// Tells the system's Now Playing (it sends only what changed).
    fn show_now_playing(&self, state: &PlayerState) {
        if let Some(now_playing) = self.0.app.try_state::<NowPlaying>() {
            now_playing.update(state, &self.0.artwork);
        }
    }

    fn emit_position(&self, force: bool) {
        {
            let mut last = lock(&self.0.last_position_emit);
            if !force && last.elapsed() < POSITION_INTERVAL {
                return;
            }
            *last = Instant::now();
        }
        let state = self.state();
        let update = PositionUpdate {
            position: state.position,
            duration: state.duration,
        };
        self.emit(POSITION_EVENT, &update);
        // A seek shows up here as a jump in the position.
        self.show_now_playing(&state);
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

/// Whether a file opened or dropped directly can be played: audio, plus the video containers
/// the library scanner skips (mpv plays their audio).
fn playable(path: &Path) -> bool {
    library::is_audio_file(path)
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "webm"))
}

/// Files to play for opened or dropped paths: playable files as they are, folders as the
/// audio files inside them (recursively, sorted by path).
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
        } else if path.is_file() && playable(&path) {
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

/// Programs mpv's yt-dlp hook runs for links.
#[derive(Debug, Clone, Default)]
struct LinkHelpers {
    ytdlp: Option<PathBuf>,
    deno: Option<PathBuf>,
}

/// Command-line flags for mpv. `--no-config` keeps the user's mpv.conf (and the bundle's
/// pseudo-GUI config) out; media keys and Now Playing belong to Vitra, not mpv.
fn mpv_args(with_newer_options: bool, helpers: &LinkHelpers) -> Vec<OsString> {
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
    // mpv looks for yt-dlp only next to itself and on PATH. `-append` takes one key=value
    // pair, so paths with commas are safe.
    if let Some(ytdlp) = &helpers.ytdlp {
        let mut arg = OsString::from("--script-opts-append=ytdl_hook-ytdl_path=");
        arg.push(ytdlp);
        args.push(arg);
    }
    if let Some(deno) = &helpers.deno {
        let mut arg = OsString::from("--ytdl-raw-options-append=js-runtimes=deno:");
        arg.push(deno);
        args.push(arg);
    }
    // Debugging aid, e.g. VITRA_MPV_ARGS="--ao=null --log-file=/tmp/mpv.log".
    if let Ok(extra) = std::env::var("VITRA_MPV_ARGS") {
        args.extend(extra.split_whitespace().map(OsString::from));
    }
    args
}

/// Reads the pulse and the spectrum while a window wants them; quieter (less often) while
/// nothing plays.
async fn read_meters(player: Player) {
    let mut envelope = level::Envelope::default();
    let mut sent_pulse = 0.0_f64;
    let mut waiting = spectrum::DelayLine::default();
    // The windows were last told that nothing plays (an empty spectrum).
    let mut spectrum_idle = false;
    let level_property = format!("af-metadata/{}", level::FILTER_LABEL);
    let spectrum_property = format!("af-metadata/{}", spectrum::FILTER_LABEL);
    loop {
        let pulse_wanted = player.0.pulse_wanted.load(Ordering::Relaxed);
        let spectrum_wanted = player.0.spectrum_wanted.load(Ordering::Relaxed);
        if !pulse_wanted && !spectrum_wanted {
            break;
        }
        let playing = lock(&player.0.state).status == Status::Playing;
        let client = player.client().ok().filter(|_| playing);

        let mut pulse = 0.0;
        if pulse_wanted {
            let reading = match &client {
                Some(client) => client
                    .get_property::<Value>(&level_property)
                    .await
                    .ok()
                    .as_ref()
                    .and_then(level::rms_db),
                None => None,
            };
            pulse = (envelope.next(reading) * 100.0).round() / 100.0;
        }
        if (pulse - sent_pulse).abs() >= 0.01 {
            sent_pulse = pulse;
            player.emit(level::LEVEL_EVENT, &pulse);
        }

        if spectrum_wanted {
            match &client {
                Some(client) => {
                    let now = Instant::now();
                    if let Some(values) = client
                        .get_property::<Value>(&spectrum_property)
                        .await
                        .ok()
                        .as_ref()
                        .and_then(spectrum::reading)
                    {
                        waiting.push(now, values);
                    }
                    if let Some(values) = waiting.take(now) {
                        player.emit(spectrum::SPECTRUM_EVENT, &values);
                        spectrum_idle = false;
                    }
                }
                None if !spectrum_idle => {
                    waiting.clear();
                    player.emit(spectrum::SPECTRUM_EVENT, &Vec::<f32>::new());
                    spectrum_idle = true;
                }
                None => {}
            }
        }

        let pause = if playing && spectrum_wanted {
            spectrum::POLL
        } else if playing || pulse > 0.0 {
            level::POLL
        } else {
            level::IDLE_POLL
        };
        tokio::time::sleep(pause).await;
    }
    player.emit(level::LEVEL_EVENT, &0.0_f64);
    player.emit(spectrum::SPECTRUM_EVENT, &Vec::<f32>::new());
    player.0.meters_running.store(false, Ordering::Relaxed);
    // A window may have asked again just as this stopped.
    let pulse = player.0.pulse_wanted.load(Ordering::Relaxed);
    let spectrum = player.0.spectrum_wanted.load(Ordering::Relaxed);
    if pulse || spectrum {
        player.set_meters(pulse, spectrum);
    }
}

async fn supervise(player: Player) {
    let paths = sidecar::search_paths(&player.0.app);
    let Some(mpv) = paths.locate(Sidecar::Mpv) else {
        log::error!("mpv not found");
        player.fail(PlayerError::MpvMissing);
        return;
    };
    log::info!("using mpv at {} ({:?})", mpv.path.display(), mpv.source);
    let helpers = LinkHelpers {
        ytdlp: paths.locate(Sidecar::YtDlp).map(|found| found.path),
        deno: paths.locate(Sidecar::Deno).map(|found| found.path),
    };

    let mut newer_options = true;
    let mut failures: VecDeque<Instant> = VecDeque::new();
    loop {
        let spawned = MpvProcess::spawn(SpawnOptions {
            binary: mpv.path.clone(),
            args: mpv_args(newer_options, &helpers),
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
    fn the_yt_dlp_hook_explains_failed_links() {
        let message = |prefix: &str, level: &str, text: &str| json!({"event": "log-message", "prefix": prefix, "level": level, "text": text});
        let cookies = "ERROR: [Errno 1] Operation not permitted: \
            '/Users/me/Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies'\n";
        let line = hook_line(&message("ytdl_hook", "error", cookies)).unwrap();
        assert!(line.ends_with("Cookies.binarycookies'"), "trimmed: {line}");
        // Other modules and chatter are left out.
        assert_eq!(hook_line(&message("cplayer", "error", "boom")), None);
        assert_eq!(hook_line(&message("ytdl_hook", "v", "status: 1")), None);

        assert!(matches!(
            link_failure(&[line]),
            Some(links::LinkError::Cookies(_))
        ));
        let age = "ERROR: [youtube] abc: Sign in to confirm your age. This video may be \
            inappropriate for some users."
            .to_owned();
        assert!(matches!(
            link_failure(&[age]),
            Some(links::LinkError::AgeRestricted)
        ));
        // Errors without a remedy keep the queue going.
        assert!(
            link_failure(&["ERROR: Unsupported URL: https://example.com".to_owned()]).is_none()
        );
        assert!(link_failure(&[]).is_none());
    }

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
        let newer = mpv_args(true, &LinkHelpers::default());
        let older = mpv_args(false, &LinkHelpers::default());
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

    #[test]
    fn links_use_our_yt_dlp_and_deno() {
        let helpers = LinkHelpers {
            ytdlp: Some(PathBuf::from("/Apps/Vitra, Inc/yt-dlp")),
            deno: Some(PathBuf::from("/bin/deno")),
        };
        let args = mpv_args(true, &helpers);
        assert!(args.contains(&"--ytdl-format=bestaudio/best".into()));
        assert!(
            args.contains(
                &"--script-opts-append=ytdl_hook-ytdl_path=/Apps/Vitra, Inc/yt-dlp".into()
            )
        );
        assert!(args.contains(&"--ytdl-raw-options-append=js-runtimes=deno:/bin/deno".into()));
        assert!(
            mpv_args(true, &LinkHelpers::default())
                .iter()
                .all(|arg| !arg.to_string_lossy().contains("ytdl_path"))
        );
    }
}
