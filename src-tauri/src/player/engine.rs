//! mpv itself: the processes, the commands sent to them, and the flags they start with.
//!
//! One supervisor per deck restarts mpv after a crash and hands its events to
//! [`super::events`]. The active deck plays the current track; the other exists only
//! while crossfade is on.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use mpv_ipc::{Client, MpvProcess, SpawnOptions};
use serde_json::{Value, json};
use tauri::Manager;

use crate::error::AppError;
use crate::settings::SettingsStore;
use crate::sidecar::{self, Sidecar};

use super::state::{OBSERVED, PlayerError, Status};
use super::{COMMAND_TIMEOUT, Decks, Player, level, lock, session, spectrum};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a command waits for mpv to (re)start before giving up.
const READY_TIMEOUT: Duration = Duration::from_secs(15);

/// Give up restarting after this many failures within [`RESTART_WINDOW`].
const MAX_FAILURES: usize = 5;

const RESTART_WINDOW: Duration = Duration::from_secs(60);

/// Programs mpv's yt-dlp hook runs for links.
#[derive(Debug, Clone, Default)]
struct LinkHelpers {
    ytdlp: Option<PathBuf>,
    deno: Option<PathBuf>,
}

impl Player {
    /// Restores the last session and starts mpv in the background.
    pub fn start(&self) {
        self.restore_session();
        self.spawn_deck(0);
        self.crossfade_changed();
        tauri::async_runtime::spawn(session::autosave(self.clone()));
    }

    pub(super) fn spawn_deck(&self, deck: usize) {
        self.0.wanted[deck].store(true, Ordering::Relaxed);
        let mut supervisors = lock(&self.0.supervisors);
        if supervisors[deck]
            .as_ref()
            .is_some_and(|handle| !handle.inner().is_finished())
        {
            return;
        }
        supervisors[deck] = Some(tauri::async_runtime::spawn(supervise(self.clone(), deck)));
    }

    /// Crossfades need the second deck; without them it isn't worth its memory.
    pub(super) fn crossfade_changed(&self) {
        let wanted = self.crossfade_seconds() > 0.0;
        let standby = 1 - self.active_deck();
        if wanted {
            self.spawn_deck(standby);
        } else if self.0.wanted[standby].swap(false, Ordering::Relaxed) {
            self.cancel_crossfade();
            self.0.stop[standby].notify_one();
        }
    }

    /// Saves the session and quits mpv; called when the app exits so no orphaned mpv keeps
    /// running.
    pub async fn shutdown(&self) {
        self.save_session(true);
        for deck in 0..2 {
            self.0.wanted[deck].store(false, Ordering::Relaxed);
            self.0.stop[deck].notify_one();
        }
        let handles = std::mem::take(&mut *lock(&self.0.supervisors));
        for handle in handles.into_iter().flatten() {
            let _ = tokio::time::timeout(Duration::from_secs(3), handle).await;
        }
    }

    /// Volume, mute and speed belong to the player, not to one deck.
    pub(super) async fn set_on_every_deck(&self, name: &str, value: Value) -> Result<(), AppError> {
        let active = self.active_deck();
        let mut result = Err(AppError::Player(
            "the playback engine is not running".into(),
        ));
        for (deck, client) in self.clients() {
            let set = command(
                &client,
                vec![json!("set_property"), json!(name), value.clone()],
            )
            .await;
            if deck == active {
                result = set;
            }
        }
        result
    }

    pub(super) async fn set_property(&self, name: &str, value: Value) -> Result<(), AppError> {
        let client = self.client()?;
        command(&client, vec![json!("set_property"), json!(name), value]).await
    }

    pub(super) fn decks(&self) -> std::sync::RwLockReadGuard<'_, Decks> {
        self.0
            .decks
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The deck playing the current track.
    pub(super) fn active_deck(&self) -> usize {
        self.decks().active
    }

    /// The mpv of the deck playing the current track.
    pub(super) fn client(&self) -> Result<Arc<Client>, AppError> {
        let decks = self.decks();
        decks.clients[decks.active]
            .clone()
            .ok_or_else(|| AppError::Player("the playback engine is not running".into()))
    }

    pub(super) fn deck_client(&self, deck: usize) -> Option<Arc<Client>> {
        self.decks().clients[deck].clone()
    }

    /// Every running mpv, for what all decks share (volume, filters, link options).
    pub(super) fn clients(&self) -> Vec<(usize, Arc<Client>)> {
        let decks = self.decks();
        (0..2)
            .filter_map(|deck| decks.clients[deck].clone().map(|client| (deck, client)))
            .collect()
    }

    pub(super) async fn wait_for_client(&self) -> Result<Arc<Client>, AppError> {
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

    pub(super) fn set_client(&self, deck: usize, client: Option<Arc<Client>>) {
        let connected = client.is_some();
        let active = {
            let mut decks = self
                .0
                .decks
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            decks.clients[deck] = client;
            decks.active == deck
        };
        if connected && active {
            self.0.ready.notify_waiters();
        }
    }

    /// Sets up a freshly started mpv: observers, volume/speed, its filters and, on the active
    /// deck, the track it was playing. The other deck waits paused.
    pub(super) async fn attach(&self, deck: usize, client: &Client) -> Result<(), AppError> {
        let state = self.state();
        let active = deck == self.active_deck();
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
            vec![
                json!("set_property"),
                json!("pause"),
                json!(state.paused || !active),
            ],
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
        self.add_equalizer(deck, client).await;
        self.apply_normalization(client).await;
        if !active {
            // A deck that restarted lost the track it had prepared or was fading out.
            {
                let mut fade = lock(&self.0.fade);
                if fade.prep.is_some_and(|prep| prep.deck == deck) {
                    fade.prep = None;
                }
                if fade.outgoing == Some(deck) {
                    fade.outgoing = None;
                }
            }
            // The track playing may blend into the next one now that this deck is up (it
            // usually comes up after the first track started).
            let player = self.clone();
            tauri::async_runtime::spawn(async move {
                let _transport = player.0.transport.lock().await;
                if let Ok(client) = player.client()
                    && let Err(err) = player.sync_next(&client, false).await
                {
                    log::warn!("cannot prepare the next track: {err}");
                }
            });
            return Ok(());
        }
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
    pub(super) async fn set_ytdl_options(&self, client: &Client) {
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

    /// The link settings (cookies) changed.
    pub fn links_settings_changed(&self) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            for (_, client) in player.clients() {
                player.set_ytdl_options(&client).await;
            }
        });
    }
}

pub(super) async fn command(client: &Client, args: Vec<Value>) -> Result<(), AppError> {
    command_value(client, args).await.map(drop)
}

pub(super) async fn command_value(client: &Client, args: Vec<Value>) -> Result<Value, AppError> {
    match tokio::time::timeout(COMMAND_TIMEOUT, client.command(args)).await {
        Ok(result) => result.map_err(mpv_error),
        Err(_) => Err(AppError::Player("mpv did not respond".into())),
    }
}

/// The playlist entry id `loadfile` reports (mpv 0.33+).
/// `loadfile` with its arguments named (their order changed in mpv 0.38) and options that
/// apply to that file only.
pub(super) async fn loadfile(
    client: &Client,
    path: &str,
    flags: &str,
    options: serde_json::Map<String, Value>,
) -> Result<Value, AppError> {
    let mut command = serde_json::Map::new();
    command.insert("name".into(), json!("loadfile"));
    command.insert("url".into(), json!(path));
    command.insert("flags".into(), json!(flags));
    if !options.is_empty() {
        command.insert("options".into(), Value::Object(options));
    }
    match tokio::time::timeout(COMMAND_TIMEOUT, client.command_named(command)).await {
        Ok(result) => result.map_err(mpv_error),
        Err(_) => Err(AppError::Player("mpv did not answer in time".into())),
    }
}

pub(super) fn entry_id(reply: &Value) -> Option<i64> {
    let id = reply.get("playlist_entry_id").and_then(Value::as_i64);
    if id.is_none() {
        log::warn!("mpv did not report a playlist entry id; gapless playback is off");
    }
    id
}

pub(super) fn mpv_error(err: mpv_ipc::Error) -> AppError {
    AppError::Player(err.to_string())
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
pub(super) async fn read_meters(player: Player) {
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

/// Runs one deck's mpv: starts it, hands its events to the player, and restarts it after a
/// crash (giving up after too many), until the deck isn't wanted any more.
async fn supervise(player: Player, deck: usize) {
    let paths = sidecar::search_paths(&player.0.app);
    let Some(mpv) = paths.locate(Sidecar::Mpv) else {
        log::error!("mpv not found");
        player.fail(PlayerError::MpvMissing);
        return;
    };
    log::info!(
        "deck {deck}: using mpv at {} ({:?})",
        mpv.path.display(),
        mpv.source
    );
    let helpers = LinkHelpers {
        ytdlp: paths.locate(Sidecar::YtDlp).map(|found| found.path),
        deno: paths.locate(Sidecar::Deno).map(|found| found.path),
    };
    let stop = &player.0.stop[deck];
    let wanted = || player.0.wanted[deck].load(Ordering::Relaxed);

    let mut newer_options = true;
    let mut failures: VecDeque<Instant> = VecDeque::new();
    while wanted() {
        let spawned = MpvProcess::spawn(SpawnOptions {
            binary: mpv.path.clone(),
            args: mpv_args(newer_options, &helpers),
            pipe_prefix: format!("vitra-mpv{deck}"),
            startup_timeout: STARTUP_TIMEOUT,
        })
        .await;

        let error = match spawned {
            Ok((mut process, client, mut events)) => {
                log::info!("deck {deck}: {} started", process.version());
                let client = Arc::new(client);
                if let Err(err) = player.attach(deck, &client).await {
                    log::warn!("setting up mpv failed: {err}");
                }
                player.set_client(deck, Some(Arc::clone(&client)));
                if deck == player.active_deck() {
                    {
                        let mut state = lock(&player.0.state);
                        if matches!(state.error, Some(PlayerError::MpvFailed { .. })) {
                            state.error = None;
                        }
                    }
                    player.emit_state();
                }

                let reason = loop {
                    tokio::select! {
                        event = events.recv() => match event {
                            Some(event) => player.handle_event(deck, event, &client).await,
                            None => break "lost the IPC connection".to_owned(),
                        },
                        status = process.wait() => break format!("exited ({status:?})"),
                        () = stop.notified() => {
                            if !wanted() {
                                player.set_client(deck, None);
                                process.shutdown(&client, Duration::from_secs(2)).await;
                                return;
                            }
                        }
                    }
                };
                player.set_client(deck, None);
                reason
            }
            Err(mpv_ipc::Error::Exited(status)) if newer_options => {
                log::warn!("mpv exited at startup ({status}); retrying without newer options");
                newer_options = false;
                continue;
            }
            Err(err) => err.to_string(),
        };

        log::warn!("deck {deck}: mpv stopped: {error}");
        if !wanted() {
            return;
        }
        let now = Instant::now();
        failures.push_back(now);
        while failures
            .front()
            .is_some_and(|t| now.duration_since(*t) > RESTART_WINDOW)
        {
            failures.pop_front();
        }
        if failures.len() >= MAX_FAILURES {
            if deck == player.active_deck() {
                player.fail(PlayerError::MpvFailed { message: error });
            }
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(failures.len() as u64)) => {}
            () = stop.notified() => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
