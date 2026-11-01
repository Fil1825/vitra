//! The player: runs mpv, mirrors its state, owns the play queue and exposes playback
//! commands.
//!
//! [`engine`] supervises the mpv processes (restarting one that dies, restoring volume,
//! speed, the queue and the position) and [`events`] forwards mpv events through
//! [`PlayerState::apply`] to the windows as `player://state` and throttled
//! `player://position` events. [`playback`] owns the queue and the transport,
//! [`metadata`] reads tags and artwork, [`blend`] crossfades on a second deck and
//! [`audio`] applies the equalizer, loudness and the meters.
//!
//! Gapless playback: mpv's own playlist holds the playing file plus the item that follows it
//! in the queue (`loadfile <next> append`), so mpv opens the next file before the current
//! one ends. Every queue change re-mirrors that one entry.

mod artwork;
mod audio;
mod blend;
mod commands;
mod crossfade;
mod engine;
mod events;
mod level;
mod metadata;
mod playback;
mod queue;
mod session;
mod spectrum;
mod state;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use mpv_ipc::Client;
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use crate::library::LibraryService;
use crate::lyrics::LyricsService;
use crate::sound::Equalizer;

pub use artwork::{
    ArtworkStore, COVER_PREFIX, SCHEME as ARTWORK_SCHEME, respond_cover, status as http_status,
};
pub use commands::*;
use queue::Queue;
pub use queue::{Entry, QueueView, Repeat};
pub use state::{PlayerState, Status};

pub const STATE_EVENT: &str = "player://state";
pub const POSITION_EVENT: &str = "player://position";
pub const TRACK_EVENT: &str = "player://track-changed";
pub const QUEUE_EVENT: &str = "queue://changed";

/// How long one command waits for mpv before the player gives up on it.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

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
    /// The mpv instances: the one playing the current track, and a second one for
    /// crossfades (see [`crossfade`]).
    decks: RwLock<Decks>,
    artwork: ArtworkStore,
    next_track_id: AtomicU64,
    last_position_emit: Mutex<Instant>,
    /// Restore this position once the reloaded file is ready (session restore, mpv restart).
    pending_seek: Mutex<Option<f64>>,
    /// Signalled whenever a new mpv connection becomes available.
    ready: Notify,
    /// Stops a deck's supervisor (the app quits, or crossfades were turned off).
    stop: [Notify; 2],
    /// Which decks should be running.
    wanted: [AtomicBool; 2],
    supervisors: Mutex<[Option<JoinHandle<()>>; 2]>,
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
    /// What mpv's yt-dlp hook logged while opening the current link: its warnings and
    /// errors, kept so a failure can say why (cookies, an age check, a sign-in).
    hook_log: Mutex<Vec<String>>,
    /// A window shows the backdrop's pulse (see [`level`]) or the spectrum (see
    /// [`spectrum`]): read them from mpv while one does.
    pulse_wanted: AtomicBool,
    spectrum_wanted: AtomicBool,
    meters_running: AtomicBool,
    /// The equalizer in each deck's chain now (default: none).
    equalizer: Mutex<[Equalizer; 2]>,
    /// Equalizer changes wait for each other.
    equalizer_turn: tokio::sync::Mutex<()>,
    fade: Mutex<Fade>,
}

/// The mpv instances, by deck number.
#[derive(Default)]
struct Decks {
    /// The deck playing the current track.
    active: usize,
    clients: [Option<Arc<Client>>; 2],
}

/// A crossfade on its way.
#[derive(Debug, Default)]
struct Fade {
    /// The next track, loaded paused and silent on the other deck.
    prep: Option<Prep>,
    /// The deck still fading the previous track out.
    outgoing: Option<usize>,
    /// The volumes are being ramped: their changes are not the user's.
    ramping: bool,
    /// A track the other deck couldn't open: it follows gapless instead.
    failed: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Prep {
    deck: usize,
    uid: u64,
    /// mpv's playlist entry of the loaded file.
    entry: Option<i64>,
    /// The file is loaded.
    ready: bool,
    /// Its start is scheduled.
    armed: bool,
    /// The playing track ended before the crossfade began: start as soon as possible, at
    /// full volume.
    due: bool,
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
            decks: RwLock::new(Decks::default()),
            artwork: ArtworkStore::default(),
            next_track_id: AtomicU64::new(1),
            last_position_emit: Mutex::new(Instant::now()),
            pending_seek: Mutex::new(None),
            ready: Notify::new(),
            stop: [Notify::new(), Notify::new()],
            wanted: [AtomicBool::new(true), AtomicBool::new(false)],
            supervisors: Mutex::new([None, None]),
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
            equalizer: Mutex::new([Equalizer::default(), Equalizer::default()]),
            equalizer_turn: tokio::sync::Mutex::new(()),
            fade: Mutex::new(Fade::default()),
        }))
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
}
