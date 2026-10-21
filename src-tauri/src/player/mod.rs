//! The player: runs mpv, mirrors its state and exposes playback commands.
//!
//! A supervisor task owns the mpv process. It restarts mpv if it dies (restoring volume,
//! speed and the current track) and forwards mpv events through [`PlayerState::apply`] to the
//! windows as `player://state` and throttled `player://position` events.

mod artwork;
mod commands;
mod state;

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, Instant};

use media::TrackMetadata;
use mpv_ipc::{Client, Event, MpvProcess, SpawnOptions};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::async_runtime::JoinHandle;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

use crate::error::AppError;
use crate::sidecar::{self, Sidecar};

pub use artwork::{ArtworkStore, SCHEME as ARTWORK_SCHEME};
pub use commands::*;
pub use state::PlayerState;
use state::{ArtworkInfo, OBSERVED, PlayerError, Status, Track};

pub const STATE_EVENT: &str = "player://state";
pub const POSITION_EVENT: &str = "player://position";
pub const TRACK_EVENT: &str = "player://track-changed";

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

struct Inner {
    app: AppHandle,
    state: Mutex<PlayerState>,
    client: RwLock<Option<Arc<Client>>>,
    artwork: ArtworkStore,
    next_track_id: AtomicU64,
    last_position_emit: Mutex<Instant>,
    /// Restore this position once the reloaded file is ready (after an mpv restart).
    pending_seek: Mutex<Option<f64>>,
    /// Signalled whenever a new mpv connection becomes available.
    ready: Notify,
    shutdown: Notify,
    supervisor: Mutex<Option<JoinHandle<()>>>,
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
        }))
    }

    /// Starts mpv in the background.
    pub fn start(&self) {
        let handle = tauri::async_runtime::spawn(supervise(self.clone()));
        *lock(&self.0.supervisor) = Some(handle);
    }

    /// Quits mpv; called when the app exits so no orphaned mpv keeps running.
    pub async fn shutdown(&self) {
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

    /// Plays a file from the start, replacing whatever is playing.
    pub async fn open(&self, path: PathBuf) -> Result<(), AppError> {
        if !path.is_file() {
            return Err(AppError::NotFound(path.display().to_string()));
        }
        let id = self.0.next_track_id.fetch_add(1, Ordering::Relaxed);
        {
            let mut state = lock(&self.0.state);
            state.track = Some(Track {
                id,
                path: path.to_string_lossy().into_owned(),
                title: file_title(&path),
                metadata: TrackMetadata::default(),
                artwork: None,
            });
            state.status = Status::Loading;
            state.position = 0.0;
            state.duration = None;
            state.error = None;
        }
        self.emit_state();
        self.read_metadata(id, path.clone());
        self.load(&path).await
    }

    async fn load(&self, path: &Path) -> Result<(), AppError> {
        // Files opened at launch arrive before mpv is up.
        let client = self.wait_for_client().await?;
        command(
            &client,
            vec![json!("set_property"), json!("pause"), json!(false)],
        )
        .await?;
        command(
            &client,
            vec![
                json!("loadfile"),
                json!(path.to_string_lossy()),
                json!("replace"),
            ],
        )
        .await
    }

    pub async fn play(&self) -> Result<(), AppError> {
        let (status, path) = {
            let state = lock(&self.0.state);
            let path = state.track.as_ref().map(|t| PathBuf::from(&t.path));
            (state.status, path)
        };
        match (status, path) {
            // The track ended: play it again from the start.
            (Status::Idle, Some(path)) => self.load(&path).await,
            _ => self.set_property("pause", json!(false)).await,
        }
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
        if matches!(event, Event::FileLoaded) {
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
        let changes = lock(&self.0.state).apply(&event);
        if changes.state {
            self.emit_state();
        } else if changes.position {
            self.emit_position(false);
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
        if let (Status::Playing | Status::Paused | Status::Loading, Some(track)) =
            (state.status, &state.track)
        {
            *lock(&self.0.pending_seek) = Some(state.position);
            command(
                client,
                vec![json!("loadfile"), json!(track.path), json!("replace")],
            )
            .await?;
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

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: &S) {
        if let Err(err) = self.0.app.emit(event, payload) {
            log::warn!("cannot emit {event}: {err}");
        }
    }
}

async fn command(client: &Client, args: Vec<Value>) -> Result<(), AppError> {
    match tokio::time::timeout(COMMAND_TIMEOUT, client.command(args)).await {
        Ok(result) => result.map(drop).map_err(mpv_error),
        Err(_) => Err(AppError::Player("mpv did not respond".into())),
    }
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
