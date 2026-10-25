//! The system's "Now Playing": macOS's widget in Control Center, on the lock screen and in
//! the menu of AirPods and other headphones, Windows' media overlay (SMTC), Linux's MPRIS.
//! It shows the playing track with its artwork and position, and its buttons, the media keys
//! and headphone buttons control the player.
//!
//! mpv's own media controls stay off (`--media-controls=no`): the queue is Vitra's, so Vitra
//! publishes the track from the player's state and handles the commands itself. The system
//! moves the position along at the playback rate, so a new position is published only when
//! the status, the rate or the track changes, or the position jumps (a seek).

#[cfg(not(target_os = "macos"))]
mod controls;
#[cfg(target_os = "macos")]
mod macos;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use media::PreparedArtwork;
use tauri::{AppHandle, Manager};

use crate::player::{ArtworkStore, Player, PlayerState, Status};

/// What the system shows about the playing track.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Info {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Seconds; none for a radio stream.
    pub duration: Option<f64>,
    /// The artwork's id in the player's [`ArtworkStore`].
    pub artwork: Option<String>,
}

impl Info {
    /// The playing track, if there is one.
    pub fn of(state: &PlayerState) -> Option<Self> {
        let track = state.track.as_ref()?;
        let metadata = &track.metadata;
        let artist = if metadata.artists.len() > 1 {
            Some(metadata.artists.join(", "))
        } else {
            metadata.artist.clone()
        };
        Some(Self {
            title: track.title.clone(),
            artist: artist.filter(|artist| !artist.is_empty()),
            album: metadata.album.clone().filter(|album| !album.is_empty()),
            duration: state
                .duration
                .or(metadata.duration)
                .filter(|seconds| seconds.is_finite() && *seconds > 0.0),
            artwork: track.artwork.as_ref().map(|artwork| artwork.id.clone()),
        })
    }
}

/// Whether and where it plays.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Playback {
    Stopped,
    Paused { position: f64 },
    Playing { position: f64, rate: f64 },
}

impl Playback {
    pub fn of(state: &PlayerState) -> Self {
        let position = state.position.max(0.0);
        match state.status {
            Status::Idle => Self::Stopped,
            Status::Playing => Self::Playing {
                position,
                rate: state.speed,
            },
            // A link that is still opening shows paused where it will start.
            Status::Loading | Status::Paused => Self::Paused { position },
        }
    }

    fn position(self) -> Option<f64> {
        match self {
            Self::Stopped => None,
            Self::Paused { position } | Self::Playing { position, .. } => Some(position),
        }
    }
}

/// How far the position may drift from where the system extrapolates it before it counts
/// as a seek. Position updates come four times a second, mpv's clock and the system's drift
/// apart by far less than this.
const SEEK_TOLERANCE: f64 = 1.0;

/// What changed since the last update.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Update {
    /// The track to show (`Some(None)`: nothing plays any more).
    pub info: Option<Option<Info>>,
    pub playback: Option<Playback>,
}

impl Update {
    pub fn is_empty(&self) -> bool {
        self.info.is_none() && self.playback.is_none()
    }
}

/// Remembers what the system was told, to tell it only what changed.
#[derive(Debug, Default)]
pub struct Publisher {
    info: Option<Info>,
    playback: Option<(Playback, Instant)>,
    /// Systems that don't move the position along themselves (MPRIS: clients read the
    /// position when they show it) get it again this often while playing.
    refresh: Option<Duration>,
}

impl Publisher {
    pub fn refreshing(every: Duration) -> Self {
        Self {
            refresh: Some(every),
            ..Self::default()
        }
    }

    pub fn next(&mut self, info: Option<Info>, playback: Playback, now: Instant) -> Update {
        let mut update = Update::default();
        let track_changed = info != self.info;
        if track_changed {
            update.info = Some(info.clone());
            self.info = info;
        }
        let moved = match self.playback {
            None => true,
            Some((last, at)) => {
                let kind_changed =
                    std::mem::discriminant(&last) != std::mem::discriminant(&playback);
                let rate_changed = matches!(
                    (last, playback),
                    (Playback::Playing { rate: a, .. }, Playback::Playing { rate: b, .. })
                        if (a - b).abs() > f64::EPSILON
                );
                let jumped = match (last, playback.position()) {
                    (Playback::Playing { position, rate }, Some(now_at)) => {
                        let expected = position + now.duration_since(at).as_secs_f64() * rate;
                        (now_at - expected).abs() > SEEK_TOLERANCE
                    }
                    (Playback::Paused { position }, Some(now_at)) => {
                        (now_at - position).abs() > SEEK_TOLERANCE / 2.0
                    }
                    _ => false,
                };
                let stale = matches!(playback, Playback::Playing { .. })
                    && self
                        .refresh
                        .is_some_and(|every| now.duration_since(at) >= every);
                kind_changed || rate_changed || jumped || stale
            }
        };
        if moved || track_changed {
            update.playback = Some(playback);
            self.playback = Some((playback, now));
        }
        update
    }
}

/// A button, key or headphone press, or a scrub in the system's controls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// To this many seconds.
    SeekTo(f64),
    /// By this many seconds, back when negative (MPRIS; macOS's widget scrubs instead).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    SeekBy(f64),
    /// Bring the window to the front (MPRIS).
    Raise,
}

/// Runs a command from the system on the player.
fn run(app: &AppHandle, command: Command) {
    if command == Command::Raise {
        if let Some(window) = app.get_webview_window(crate::mini::MAIN) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        return;
    }
    let Some(player) = app
        .try_state::<Player>()
        .map(|player| player.inner().clone())
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let result = match command {
            Command::Play => player.play().await,
            Command::Pause => player.pause().await,
            Command::Toggle => player.toggle().await,
            Command::Next => player.next().await,
            Command::Previous => player.previous().await,
            Command::Stop => player.pause().await,
            Command::SeekTo(position) => player.seek(position.max(0.0)).await,
            Command::SeekBy(seconds) => {
                let position = player.state().position;
                player.seek((position + seconds).max(0.0)).await
            }
            Command::Raise => Ok(()),
        };
        if let Err(err) = result {
            log::info!("now playing: {command:?} failed: {err}");
        }
    });
}

/// A platform's way of showing the track.
trait Backend: Send {
    fn set_info(&mut self, info: Option<&Info>, artwork: Option<Arc<PreparedArtwork>>);
    fn set_playback(&mut self, playback: Playback);
    /// How often the position must be sent while playing, if the system doesn't move it
    /// along by itself.
    fn refresh(&self) -> Option<Duration> {
        None
    }
}

struct Inner {
    publisher: Publisher,
    backend: Option<Box<dyn Backend>>,
}

/// The app's entry in the system's Now Playing.
#[derive(Clone)]
pub struct NowPlaying(Arc<Mutex<Inner>>);

impl NowPlaying {
    /// Registers with the system; commands go to the [`Player`] from then on.
    pub fn new(app: &AppHandle) -> Self {
        let handler = {
            let app = app.clone();
            move |command| run(&app, command)
        };
        #[cfg(target_os = "macos")]
        let backend = macos::start(app.clone(), handler);
        #[cfg(not(target_os = "macos"))]
        let backend = controls::start(app, handler);
        let backend = backend
            .inspect_err(|err| log::warn!("no system media controls: {err}"))
            .ok();
        let publisher = match backend.as_ref().and_then(|backend| backend.refresh()) {
            Some(every) => Publisher::refreshing(every),
            None => Publisher::default(),
        };
        Self(Arc::new(Mutex::new(Inner { publisher, backend })))
    }

    /// The player's state changed (or its position moved on).
    pub fn update(&self, state: &PlayerState, artwork: &ArtworkStore) {
        let info = Info::of(state);
        let playback = if info.is_some() {
            Playback::of(state)
        } else {
            Playback::Stopped
        };
        let mut inner = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let update = inner.publisher.next(info, playback, Instant::now());
        if update.is_empty() {
            return;
        }
        let Some(backend) = inner.backend.as_mut() else {
            return;
        };
        if let Some(info) = &update.info {
            let image = info
                .as_ref()
                .and_then(|info| info.artwork.as_deref())
                .and_then(|id| artwork.get(id));
            backend.set_info(info.as_ref(), image);
        }
        if let Some(playback) = update.playback {
            backend.set_playback(playback);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(title: &str) -> Option<Info> {
        Some(Info {
            title: title.into(),
            ..Info::default()
        })
    }

    #[test]
    fn a_new_track_sends_everything() {
        let mut publisher = Publisher::default();
        let start = Instant::now();
        let update = publisher.next(info("A"), Playback::Paused { position: 0.0 }, start);
        assert_eq!(update.info, Some(info("A")));
        assert_eq!(update.playback, Some(Playback::Paused { position: 0.0 }));
        assert!(
            publisher
                .next(info("A"), Playback::Paused { position: 0.0 }, start)
                .is_empty()
        );
        let update = publisher.next(info("B"), Playback::Paused { position: 0.0 }, start);
        assert_eq!(update.info, Some(info("B")));
        assert!(update.playback.is_some());
    }

    #[test]
    fn the_system_moves_the_position_along_by_itself() {
        let mut publisher = Publisher::default();
        let start = Instant::now();
        let playing = |position| Playback::Playing {
            position,
            rate: 1.0,
        };
        publisher.next(info("A"), playing(10.0), start);
        // Ten seconds later at the expected place: nothing to send.
        let later = start + Duration::from_secs(10);
        assert!(publisher.next(info("A"), playing(20.2), later).is_empty());
        // A seek is sent.
        let update = publisher.next(info("A"), playing(95.0), later);
        assert_eq!(update.playback, Some(playing(95.0)));
        assert_eq!(update.info, None);
        // So is a pause, a resume, a new rate.
        let update = publisher.next(info("A"), Playback::Paused { position: 95.0 }, later);
        assert_eq!(update.playback, Some(Playback::Paused { position: 95.0 }));
        let update = publisher.next(info("A"), playing(95.0), later);
        assert!(update.playback.is_some());
        let update = publisher.next(
            info("A"),
            Playback::Playing {
                position: 95.0,
                rate: 1.5,
            },
            later,
        );
        assert!(update.playback.is_some());
    }

    #[test]
    fn systems_that_dont_move_the_position_get_it_again() {
        let mut publisher = Publisher::refreshing(Duration::from_secs(1));
        let start = Instant::now();
        let playing = |position| Playback::Playing {
            position,
            rate: 1.0,
        };
        publisher.next(info("A"), playing(0.0), start);
        let soon = start + Duration::from_millis(400);
        assert!(publisher.next(info("A"), playing(0.4), soon).is_empty());
        let later = start + Duration::from_millis(1100);
        assert_eq!(
            publisher.next(info("A"), playing(1.1), later).playback,
            Some(playing(1.1))
        );
        // Not while paused.
        let paused = Playback::Paused { position: 1.1 };
        publisher.next(info("A"), paused, later);
        assert!(
            publisher
                .next(info("A"), paused, later + Duration::from_secs(5))
                .is_empty()
        );
    }

    #[test]
    fn a_paused_track_reports_seeks_only() {
        let mut publisher = Publisher::default();
        let start = Instant::now();
        publisher.next(info("A"), Playback::Paused { position: 30.0 }, start);
        let later = start + Duration::from_secs(60);
        assert!(
            publisher
                .next(info("A"), Playback::Paused { position: 30.1 }, later)
                .is_empty()
        );
        assert!(
            publisher
                .next(info("A"), Playback::Paused { position: 12.0 }, later)
                .playback
                .is_some()
        );
    }

    #[test]
    fn stopping_clears_the_track() {
        let mut publisher = Publisher::default();
        let start = Instant::now();
        publisher.next(info("A"), Playback::Paused { position: 3.0 }, start);
        let update = publisher.next(None, Playback::Stopped, start);
        assert_eq!(update.info, Some(None));
        assert_eq!(update.playback, Some(Playback::Stopped));
    }
}
