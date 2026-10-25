//! My Wave (stage 10): the library measured in the background (`analysis`) and the endless
//! queue that picks from those measurements by mood (`mood::Wave`). The queue is the
//! player's, flagged as a wave (`Queue::wave`); this service keeps a few tracks ahead of the
//! playing one, steers the wave by what is listened to, skipped, liked and disliked, and
//! saves its course with the session.

mod analysis;
mod commands;
mod process;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use library::Mark;
use mood::{Mode, Pool, Wave};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::library::{Change, LibraryService};
use crate::player::{Entry, Player};

pub use analysis::{AnalysisStatus, Analyzer};
pub use commands::*;

/// Tracks kept queued after the playing one.
pub const AHEAD: usize = 3;
/// The candidates are read again after this (new measurements, plays, favorites).
const POOL_AGE: Duration = Duration::from_secs(120);
/// The wave's course, saved with the session.
const SESSION_KEY: &str = "wave.state";

/// How starting a wave went, for the UI to explain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StartOutcome {
    Started,
    /// No track was measured yet (the analysis has just begun).
    NothingMeasured,
    /// "Like this track", but the track couldn't be measured (offline, no FFmpeg).
    TrackUnmeasured,
}

struct Built {
    at: Instant,
    links: bool,
    /// Read again before the next pick (still fine for steering meanwhile).
    stale: bool,
    pool: Arc<Pool>,
}

struct Inner {
    app: AppHandle,
    analyzer: Analyzer,
    /// The running wave (loaded from the session when a restored queue needs it).
    wave: Mutex<Option<Wave>>,
    pool: Mutex<Option<Built>>,
    /// One pick at a time: starting a wave and topping it up.
    picking: tokio::sync::Mutex<()>,
    /// A top-up runs / another was asked for meanwhile.
    filling: AtomicBool,
    again: AtomicBool,
}

#[derive(Clone)]
pub struct WaveService(Arc<Inner>);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

/// A file that isn't there (an unplugged drive) would only fail to play.
fn playable(entry: &Entry) -> bool {
    links::is_remote(&entry.path) || Path::new(&entry.path).is_file()
}

impl WaveService {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(Inner {
            analyzer: Analyzer::new(app.clone()),
            app,
            wave: Mutex::new(None),
            pool: Mutex::new(None),
            picking: tokio::sync::Mutex::new(()),
            filling: AtomicBool::new(false),
            again: AtomicBool::new(false),
        }))
    }

    pub fn start(&self) {
        self.0.analyzer.start();
    }

    /// Quitting: no FFmpeg is left behind.
    pub fn stop(&self) {
        self.0.analyzer.stop();
    }

    /// Something the analysis waits on may have changed (its settings, power saving).
    pub fn settings_changed(&self) {
        self.0.analyzer.wake();
        // Links may be in or out now.
        self.invalidate();
    }

    pub fn analysis(&self) -> AnalysisStatus {
        self.0.analyzer.status()
    }

    fn library(&self) -> Result<LibraryService, AppError> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|l| l.inner().clone())
            .ok_or_else(|| AppError::Player("the library is not open".into()))
    }

    fn player(&self) -> Result<Player, AppError> {
        self.0
            .app
            .try_state::<Player>()
            .map(|p| p.inner().clone())
            .ok_or_else(|| AppError::Player("the player is not running".into()))
    }

    /// Favorites, dislikes, new measurements or links coming and going: the candidates are
    /// read again before the next pick.
    fn invalidate(&self) {
        if let Some(built) = lock(&self.0.pool).as_mut() {
            built.stale = true;
        }
    }

    /// The measured tracks the wave may pick from now: links only while they can be reached.
    async fn pool(&self) -> Result<Arc<Pool>, AppError> {
        let links = !self.0.analyzer.offline();
        if let Some(built) = &*lock(&self.0.pool)
            && built.links == links
            && !built.stale
            && built.at.elapsed() < POOL_AGE
        {
            return Ok(built.pool.clone());
        }
        let now = library::now();
        let (candidates, habits) = self
            .library()?
            .run(move |db| Ok((db.wave_candidates()?, db.wave_habits(now)?)))
            .await?;
        let candidates = candidates
            .into_iter()
            .filter(|c| links || !c.is_link())
            .map(|c| c.candidate)
            .collect();
        let pool = tauri::async_runtime::spawn_blocking(move || Pool::new(candidates, habits));
        let pool = Arc::new(pool.await?);
        *lock(&self.0.pool) = Some(Built {
            at: Instant::now(),
            links,
            stale: false,
            pool: pool.clone(),
        });
        Ok(pool)
    }

    /// The wave of a queue restored from the last session: its saved course if it is the
    /// same mood, else a fresh one.
    async fn restore(&self, mode: Mode, pool: &Pool) -> Wave {
        let saved = match self.library() {
            Ok(library) => library
                .run(|db| db.session_get(SESSION_KEY))
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };
        saved
            .and_then(|json| serde_json::from_str::<Wave>(&json).ok())
            .filter(|wave| wave.mode == mode)
            .unwrap_or_else(|| Wave::new(mode, pool, seed()))
    }

    /// The pool, with the wave of this mood loaded.
    async fn prepare(&self, mode: Mode) -> Result<Arc<Pool>, AppError> {
        let pool = self.pool().await?;
        let loaded = lock(&self.0.wave)
            .as_ref()
            .is_some_and(|wave| wave.mode == mode);
        if !loaded {
            let wave = self.restore(mode, &pool).await;
            *lock(&self.0.wave) = Some(wave);
        }
        Ok(pool)
    }

    fn save(&self) {
        let Some(json) = lock(&self.0.wave)
            .as_ref()
            .and_then(|wave| serde_json::to_string(wave).ok())
        else {
            return;
        };
        let Ok(library) = self.library() else {
            return;
        };
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(err) = library.with(|db| db.session_set(SESSION_KEY, &json)) {
                log::warn!("cannot save the wave: {err}");
            }
        });
    }

    /// Up to `count` playable picks, as queue entries.
    async fn picks(&self, pool: &Pool, count: usize) -> Result<Vec<Entry>, AppError> {
        let library = self.library()?;
        let mut entries = Vec::new();
        // Missing files are passed over; a few rounds find others.
        for _ in 0..3 {
            let ids: Vec<i64> = {
                let mut wave = lock(&self.0.wave);
                let Some(wave) = wave.as_mut() else {
                    break;
                };
                let now = library::now();
                (entries.len()..count)
                    .map_while(|_| wave.pick(pool, now).map(|pick| pick.id))
                    .collect()
            };
            if ids.is_empty() {
                break;
            }
            entries.extend(library.entries(ids).await?.into_iter().filter(playable));
            if entries.len() >= count {
                break;
            }
        }
        Ok(entries)
    }

    /// Starts a wave in this mood. "Like this track" while that track plays keeps it
    /// playing; otherwise the wave's first pick plays at once.
    pub async fn begin(&self, mode: Mode) -> Result<StartOutcome, AppError> {
        let _picking = self.0.picking.lock().await;
        let player = self.player()?;
        let mut seed_entry = None;
        let mut from_current = false;
        if let Mode::LikeTrack(id) = mode {
            if !self.0.analyzer.measure_now(id).await {
                return Ok(StartOutcome::TrackUnmeasured);
            }
            from_current = player
                .state()
                .track
                .as_ref()
                .is_some_and(|t| t.library_id == Some(id));
            if !from_current {
                seed_entry = self.library()?.entries(vec![id]).await?.into_iter().next();
            }
            // It was just measured: read the candidates again.
            self.invalidate();
        }
        let pool = self.pool().await?;
        if pool.is_empty() {
            return Ok(StartOutcome::NothingMeasured);
        }
        *lock(&self.0.wave) = Some(Wave::new(mode, &pool, seed()));
        let mut entries: Vec<Entry> = seed_entry.into_iter().collect();
        let wanted = AHEAD + usize::from(!from_current) - entries.len();
        entries.extend(self.picks(&pool, wanted).await?);
        if entries.is_empty() {
            return Ok(StartOutcome::NothingMeasured);
        }
        self.save();
        player.play_wave(entries, mode, from_current).await?;
        Ok(StartOutcome::Started)
    }

    /// The queue is a wave and runs short: picks more, in the background. Calls while one
    /// runs make it look again when it is done.
    pub fn top_up(&self) {
        self.0.again.store(true, Ordering::Release);
        if self.0.filling.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            while service.0.again.swap(false, Ordering::AcqRel) {
                if let Err(err) = service.fill().await {
                    log::warn!("cannot top up the wave: {err}");
                }
            }
            service.0.filling.store(false, Ordering::Release);
            // Asked between the last look and now.
            if service.0.again.load(Ordering::Acquire) {
                service.top_up();
            }
        });
    }

    async fn fill(&self) -> Result<(), AppError> {
        let _picking = self.0.picking.lock().await;
        let player = self.player()?;
        let view = player.queue();
        let Some(mode) = view.wave else {
            return Ok(());
        };
        let upcoming = match view.current {
            Some(index) => view.items.len() - index - 1,
            None => view.items.len(),
        };
        if upcoming >= AHEAD {
            return Ok(());
        }
        let pool = self.prepare(mode).await?;
        let entries = self.picks(&pool, AHEAD - upcoming).await?;
        if entries.is_empty() {
            log::info!("the wave has nothing left to pick");
            return Ok(());
        }
        self.save();
        player.append_wave(entries).await?;
        Ok(())
    }

    /// Steers the running wave and saves its course. The pool cached by the last pick is
    /// fine for where tracks are, so this happens at once, before the top-up that follows a
    /// skip.
    fn steer(&self, mode: Mode, apply: impl FnOnce(&mut Wave, &Pool) + Send + 'static) {
        let pool = lock(&self.0.pool).as_ref().map(|b| b.pool.clone());
        let apply = if let Some(pool) = pool {
            let mut wave = lock(&self.0.wave);
            match wave.as_mut().filter(|wave| wave.mode == mode) {
                Some(wave) => {
                    apply(wave, &pool);
                    None
                }
                None => Some(apply),
            }
        } else {
            Some(apply)
        };
        let Some(apply) = apply else {
            self.save();
            return;
        };
        // A wave restored from the last session: loaded first.
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let _picking = service.0.picking.lock().await;
            match service.prepare(mode).await {
                Ok(pool) => {
                    if let Some(wave) = lock(&service.0.wave).as_mut() {
                        apply(wave, &pool);
                    }
                    service.save();
                }
                Err(err) => log::warn!("cannot load the wave: {err}"),
            }
        });
    }

    fn wave_mode(&self) -> Option<Mode> {
        self.player().ok().and_then(|player| player.queue().wave)
    }

    /// A track was skipped early (counted against it in every wave to come).
    pub fn skipped(&self, track_id: i64, in_wave: bool) {
        if let Ok(library) = self.library() {
            tauri::async_runtime::spawn_blocking(move || {
                let mark = Mark::Skip { in_wave };
                if let Err(err) = library.with(|db| db.mark(track_id, mark, library::now())) {
                    log::warn!("cannot note a skip: {err}");
                }
            });
        }
        if in_wave && let Some(mode) = self.wave_mode() {
            self.steer(mode, move |wave, pool| wave.skipped(pool, track_id));
        }
    }

    /// A track of the wave was listened to.
    pub fn listened(&self, track_id: i64) {
        if let Some(mode) = self.wave_mode() {
            self.steer(mode, move |wave, pool| wave.listened(pool, track_id));
        }
    }

    /// The heart: a favorite is a like for the running wave.
    pub fn liked(&self, track_id: i64) {
        self.invalidate();
        if let Some(mode) = self.wave_mode() {
            self.steer(mode, move |wave, pool| wave.liked(pool, track_id));
        }
    }

    /// "Don't like": never in a wave again (until it is liked), no longer a favorite, and
    /// skipped if it plays.
    pub async fn dislike(&self, track_id: i64) -> Result<(), AppError> {
        let library = self.library()?;
        library
            .run(move |db| {
                db.mark(track_id, Mark::Dislike, library::now())?;
                db.set_favorite(track_id, false)
            })
            .await?;
        library.changed(Change::Tracks);
        self.invalidate();
        if let Some(mode) = self.wave_mode() {
            self.steer(mode, move |wave, pool| wave.disliked(pool, track_id));
        }
        let player = self.player()?;
        let playing = player
            .state()
            .track
            .as_ref()
            .is_some_and(|t| t.library_id == Some(track_id));
        if playing {
            player.next().await?;
        }
        Ok(())
    }

    /// The wave ends; its queue stays.
    pub fn end(&self) -> Result<(), AppError> {
        self.player()?.end_wave();
        Ok(())
    }
}
