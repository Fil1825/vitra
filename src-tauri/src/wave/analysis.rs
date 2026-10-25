//! Measures the library in the background for My Wave: every file once (three slices, a
//! second or two of FFmpeg at the lowest priority), then the links, slowly (yt-dlp finds a
//! stream, FFmpeg reads 75 s from its middle, then a pause of tens of seconds; after a
//! refusal, much longer). Links the wave is about to need jump the queue. It waits while a
//! scan runs, with power saving on, and when the user pauses it.

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use library::{AnalysisJob, AnalysisProgress, FeatureSource};
use links::{LinkError, Outage, YtDlp};
use mood::ffmpeg::{file_args, slices, stream_args, stream_slice};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::Notify;

use super::process::{self, Running};
use crate::library::{CHANGED_EVENT, LibraryService, SCAN_EVENT};
use crate::links::LinkService;
use crate::settings::SettingsStore;
use crate::sidecar::{self, Sidecar};

pub const PROGRESS_EVENT: &str = "wave://progress";

/// Startup has enough to do: the analysis begins a little later.
const START_DELAY: Duration = Duration::from_secs(15);
/// A breath between files, so the analysis never takes a whole core for long.
const BETWEEN_FILES: Duration = Duration::from_millis(400);
/// Between two links: a random pause in this range, so the site sees a listener, not a bot.
const LINK_PAUSE_SECONDS: (u64, u64) = (20, 60);
/// After the site refused (too many requests, a 403), links wait this long, doubling.
const FIRST_BACKOFF: Duration = Duration::from_secs(15 * 60);
const LONGEST_BACKOFF: Duration = Duration::from_secs(6 * 3600);
/// Offline: try a link again after this.
const OFFLINE_RETRY: Duration = Duration::from_secs(5 * 60);
/// With nothing to do, look again this often (new tracks also wake it).
const IDLE_RECHECK: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisState {
    #[default]
    Idle,
    Analyzing,
    /// The user paused it.
    Paused,
    /// A library scan runs: the disk is busy with that.
    Scanning,
    PowerSaving,
    /// No FFmpeg to decode with.
    NoFfmpeg,
}

/// Where the links' analysis stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinksState {
    /// Turned off in Settings.
    Off,
    /// Its turn comes after the files, or after a pause.
    #[default]
    Waiting,
    /// The site refused: links wait (`linksRetryAt`).
    RateLimited,
    /// The site can't be reached: links wait, and the wave keeps to files.
    Offline,
    /// No yt-dlp to find streams with.
    NoYtDlp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisStatus {
    #[serde(flatten)]
    pub progress: AnalysisProgress,
    pub state: AnalysisState,
    /// The track being measured.
    pub current: Option<String>,
    pub links: LinksState,
    /// When links go on (Unix ms), while they wait after a refusal or offline.
    pub links_retry_at: Option<u64>,
    /// Files left out this session: missing (an unplugged drive) or kept in iCloud only.
    pub skipped: u32,
}

/// How measuring one link went, for the pace of the next.
enum LinkOutcome {
    Done,
    Outage(Outage),
    NoYtDlp,
}

struct Inner {
    app: AppHandle,
    wake: Notify,
    stopping: AtomicBool,
    running: Running,
    /// A measurement the wave waits for, beside the background one.
    urgent: Running,
    status: Mutex<AnalysisStatus>,
    /// Missing or iCloud-only files, left alone until the next launch.
    skipped: Mutex<HashSet<i64>>,
    /// Links the wave needs soon, before any other link.
    wanted: Mutex<VecDeque<i64>>,
    /// Links that can't be read with the chosen browser's cookies, this session.
    unreadable: Mutex<HashSet<i64>>,
}

#[derive(Clone)]
pub struct Analyzer(Arc<Inner>);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn unix_ms(at: Instant) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    now + at.saturating_duration_since(Instant::now()).as_millis() as u64
}

/// A pause between `min` and `max` seconds, different every time.
fn jitter((min, max): (u64, u64)) -> Duration {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::from(d.subsec_nanos()));
    let mixed = nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 32;
    Duration::from_secs(min + mixed % (max - min + 1))
}

impl Analyzer {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(Inner {
            app,
            wake: Notify::new(),
            stopping: AtomicBool::new(false),
            running: Running::default(),
            urgent: Running::default(),
            status: Mutex::new(AnalysisStatus::default()),
            skipped: Mutex::new(HashSet::new()),
            wanted: Mutex::new(VecDeque::new()),
            unreadable: Mutex::new(HashSet::new()),
        }))
    }

    /// Starts the background loop; library changes and finished scans wake it.
    pub fn start(&self) {
        for event in [CHANGED_EVENT, SCAN_EVENT] {
            let analyzer = self.clone();
            self.0.app.listen_any(event, move |_| analyzer.wake());
        }
        let analyzer = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(START_DELAY).await;
            analyzer.run().await;
        });
    }

    pub fn wake(&self) {
        self.0.wake.notify_one();
    }

    /// Stops at once (quitting): the running FFmpeg is killed.
    pub fn stop(&self) {
        self.0.stopping.store(true, Ordering::Release);
        self.0.running.kill();
        self.0.urgent.kill();
        self.wake();
    }

    pub fn status(&self) -> AnalysisStatus {
        lock(&self.0.status).clone()
    }

    /// Whether links can't be reached now: the wave then picks files only.
    pub fn offline(&self) -> bool {
        lock(&self.0.status).links == LinksState::Offline
    }

    /// Measures these links before any other (the wave's next candidates).
    #[allow(dead_code, reason = "the wave's queue (next) asks it")]
    pub fn want(&self, track_ids: &[i64]) {
        {
            let mut wanted = lock(&self.0.wanted);
            for id in track_ids.iter().rev() {
                wanted.retain(|known| known != id);
                wanted.push_front(*id);
            }
        }
        self.wake();
    }

    fn stopping(&self) -> bool {
        self.0.stopping.load(Ordering::Acquire)
    }

    fn library(&self) -> Option<LibraryService> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|l| l.inner().clone())
    }

    /// Waits `duration`, or less if something wakes it.
    async fn rest(&self, duration: Duration) {
        tokio::select! {
            () = tokio::time::sleep(duration) => {}
            () = self.0.wake.notified() => {}
        }
    }

    async fn publish(&self, update: impl FnOnce(&mut AnalysisStatus)) {
        let progress = match self.library() {
            Some(library) => library
                .run(|db| db.analysis_progress(mood::VERSION))
                .await
                .ok(),
            None => None,
        };
        let status = {
            let mut status = lock(&self.0.status);
            let mut next = status.clone();
            if let Some(progress) = progress {
                next.progress = progress;
            }
            next.skipped = lock(&self.0.skipped).len() as u32;
            update(&mut next);
            if next == *status {
                return;
            }
            *status = next.clone();
            next
        };
        if let Err(err) = self.0.app.emit(PROGRESS_EVENT, status) {
            log::warn!("cannot emit {PROGRESS_EVENT}: {err}");
        }
    }

    async fn run(self) {
        let mut links_at = Instant::now();
        let mut backoff = 0u32;
        while !self.stopping() {
            let (analyze, links_on, power_saving) = self
                .0
                .app
                .try_state::<SettingsStore>()
                .map(|s| {
                    let s = s.get();
                    (
                        s.wave.analyze,
                        s.wave.analyze_links,
                        s.appearance.power_saving,
                    )
                })
                .unwrap_or((true, true, false));
            let Some(library) = self.library() else {
                return;
            };
            let gate = if !analyze {
                Some(AnalysisState::Paused)
            } else if power_saving {
                Some(AnalysisState::PowerSaving)
            } else if library.scan_status().scanning {
                Some(AnalysisState::Scanning)
            } else {
                None
            };
            if let Some(state) = gate {
                self.publish(|s| {
                    s.state = state;
                    s.current = None;
                })
                .await;
                // Settings changes and the scan's end wake it sooner.
                self.rest(IDLE_RECHECK).await;
                continue;
            }
            let Some(ffmpeg) = sidecar::search_paths(&self.0.app)
                .locate(Sidecar::Ffmpeg)
                .map(|found| found.path)
            else {
                self.publish(|s| s.state = AnalysisState::NoFfmpeg).await;
                self.rest(IDLE_RECHECK).await;
                continue;
            };
            let links_ready = links_on && Instant::now() >= links_at;

            // The wave's links first, then the files, then the other links.
            let mut job = None;
            if links_ready {
                job = self.next_wanted(&library).await;
            }
            let file = job.is_none();
            if file {
                job = self.next_file(&library).await;
            }
            if job.is_none() && links_ready {
                job = self.next_link(&library).await;
            }
            let Some(job) = job else {
                self.publish(|s| {
                    s.state = AnalysisState::Idle;
                    s.current = None;
                    s.links = if links_on {
                        s.links.max_waiting()
                    } else {
                        LinksState::Off
                    };
                })
                .await;
                let wait = if links_on {
                    links_at
                        .saturating_duration_since(Instant::now())
                        .clamp(Duration::from_secs(1), IDLE_RECHECK)
                } else {
                    IDLE_RECHECK
                };
                self.rest(wait).await;
                continue;
            };
            let title = job.title.clone();
            self.publish(|s| {
                s.state = AnalysisState::Analyzing;
                s.current = Some(title);
                if !links_on {
                    s.links = LinksState::Off;
                }
            })
            .await;
            if links::is_remote(&job.path) {
                match self
                    .measure_link(&library, &ffmpeg, job, &self.0.running)
                    .await
                {
                    LinkOutcome::Done => {
                        backoff = 0;
                        links_at = Instant::now() + jitter(LINK_PAUSE_SECONDS);
                        self.publish(|s| {
                            s.links = LinksState::Waiting;
                            s.links_retry_at = None;
                        })
                        .await;
                    }
                    LinkOutcome::Outage(outage) => {
                        backoff += 1;
                        let (wait, state) = match outage {
                            Outage::RateLimited => (
                                FIRST_BACKOFF
                                    .saturating_mul(1 << (backoff - 1).min(5))
                                    .min(LONGEST_BACKOFF),
                                LinksState::RateLimited,
                            ),
                            Outage::Unreachable => (OFFLINE_RETRY, LinksState::Offline),
                        };
                        links_at = Instant::now() + wait;
                        let retry = unix_ms(links_at);
                        self.publish(|s| {
                            s.links = state;
                            s.links_retry_at = Some(retry);
                        })
                        .await;
                    }
                    LinkOutcome::NoYtDlp => {
                        links_at = Instant::now() + IDLE_RECHECK;
                        self.publish(|s| s.links = LinksState::NoYtDlp).await;
                    }
                }
            } else {
                self.measure_file(&library, &ffmpeg, job, &self.0.running)
                    .await;
                tokio::time::sleep(BETWEEN_FILES).await;
            }
        }
    }

    async fn next_wanted(&self, library: &LibraryService) -> Option<AnalysisJob> {
        loop {
            let id = lock(&self.0.wanted).pop_front()?;
            let job = library
                .run(move |db| db.analysis_job(id))
                .await
                .ok()
                .flatten();
            if let Some(job) = job.filter(|job| links::is_remote(&job.path)) {
                let known = library.run(move |db| db.features(id)).await.ok().flatten();
                if known.is_none() {
                    return Some(job);
                }
            }
        }
    }

    async fn next_file(&self, library: &LibraryService) -> Option<AnalysisJob> {
        let skipped = lock(&self.0.skipped).clone();
        let limit = 8 + skipped.len() as i64;
        let jobs = library
            .run(move |db| db.files_to_analyze(mood::VERSION, limit))
            .await
            .ok()?;
        jobs.into_iter()
            .find(|job| !skipped.contains(&job.track_id))
    }

    async fn next_link(&self, library: &LibraryService) -> Option<AnalysisJob> {
        let unreadable = lock(&self.0.unreadable).clone();
        let limit = 4 + unreadable.len() as i64;
        let jobs = library
            .run(move |db| db.links_to_analyze(mood::VERSION, limit))
            .await
            .ok()?;
        jobs.into_iter()
            .find(|job| !unreadable.contains(&job.track_id))
    }

    async fn store(
        &self,
        library: &LibraryService,
        job: &AnalysisJob,
        source: FeatureSource,
        measured: Result<mood::Features, String>,
    ) {
        let (id, stamp) = (job.track_id, job.stamp);
        if let Err(error) = &measured {
            log::info!("analysis of {}: {error}", job.path);
        }
        let stored = library
            .run(move |db| {
                db.set_features(id, mood::VERSION, source, stamp, measured, library::now())
            })
            .await;
        if let Err(err) = stored {
            log::warn!("cannot store the analysis of {}: {err}", job.path);
        }
    }

    /// Measures one track now, beside the background work (a wave "like this track" whose
    /// track wasn't measured yet). Whether its sound is known afterwards.
    pub async fn measure_now(&self, track_id: i64) -> bool {
        let Some(library) = self.library() else {
            return false;
        };
        let known = |library: LibraryService| async move {
            library
                .run(move |db| db.features(track_id))
                .await
                .ok()
                .flatten()
                .is_some()
        };
        if known(library.clone()).await {
            return true;
        }
        let Some(job) = library
            .run(move |db| db.analysis_job(track_id))
            .await
            .ok()
            .flatten()
        else {
            return false;
        };
        let Some(ffmpeg) = sidecar::search_paths(&self.0.app)
            .locate(Sidecar::Ffmpeg)
            .map(|found| found.path)
        else {
            return false;
        };
        if links::is_remote(&job.path) {
            let links_on = self
                .0
                .app
                .try_state::<SettingsStore>()
                .is_none_or(|s| s.get().wave.analyze_links);
            if !links_on || self.offline() {
                return false;
            }
            self.measure_link(&library, &ffmpeg, job, &self.0.urgent)
                .await;
        } else {
            self.measure_file(&library, &ffmpeg, job, &self.0.urgent)
                .await;
        }
        known(library).await
    }

    async fn measure_file(
        &self,
        library: &LibraryService,
        ffmpeg: &Path,
        job: AnalysisJob,
        running: &Running,
    ) {
        let path = PathBuf::from(&job.path);
        // An unplugged drive, or a file kept only in iCloud: not now, and nothing stored,
        // so it is measured once it is here.
        if !path.exists() || process::offloaded(&path) {
            lock(&self.0.skipped).insert(job.track_id);
            return;
        }
        let args = file_args(path.as_os_str(), &slices(job.duration));
        let measured = match process::decode(ffmpeg, args, running).await {
            Ok(bytes) => analyze(bytes).await,
            Err(_) if self.stopping() => return,
            Err(error) => Err(error),
        };
        self.store(library, &job, FeatureSource::File, measured)
            .await;
    }

    /// yt-dlp for this link: plain first (YouTube rarely needs cookies, and reading them
    /// can fail), with the chosen browser's cookies when the site wants a signed-in user.
    async fn stream(&self, url: &str) -> Result<links::AudioStream, LinkError> {
        let Some(links) = self.0.app.try_state::<LinkService>() else {
            return Err(LinkError::YtDlpMissing);
        };
        let signed_in = links.ytdlp().map_err(|_| LinkError::YtDlpMissing)?;
        let plain = YtDlp {
            cookies_from_browser: None,
            ..signed_in.clone()
        };
        match plain.audio_stream(url).await {
            Err(LinkError::SignInRequired | LinkError::AgeRestricted)
                if signed_in.cookies_from_browser.is_some() =>
            {
                signed_in.audio_stream(url).await
            }
            result => result,
        }
    }

    async fn measure_link(
        &self,
        library: &LibraryService,
        ffmpeg: &Path,
        job: AnalysisJob,
        running: &Running,
    ) -> LinkOutcome {
        let stream = match self.stream(&job.path).await {
            Ok(stream) => stream,
            Err(LinkError::YtDlpMissing) => return LinkOutcome::NoYtDlp,
            Err(err) => {
                if let Some(outage) = err.outage() {
                    return LinkOutcome::Outage(outage);
                }
                if matches!(err, LinkError::Cookies(_)) {
                    // Not the link's fault: left for this session.
                    lock(&self.0.unreadable).insert(job.track_id);
                } else {
                    self.store(library, &job, FeatureSource::Stream, Err(err.to_string()))
                        .await;
                }
                return LinkOutcome::Done;
            }
        };
        let slice = stream_slice(stream.duration.or(job.duration));
        let args = stream_args(&stream.url, &stream.headers, slice);
        let measured = match process::decode(ffmpeg, args, running).await {
            Ok(bytes) => analyze(bytes).await,
            Err(_) if self.stopping() => return LinkOutcome::Done,
            Err(error) => {
                // A media server's refusal is the site throttling us, not a broken link.
                if error.contains("403") || error.contains("429") {
                    return LinkOutcome::Outage(Outage::RateLimited);
                }
                if let Some(outage) = links::ytdlp::outage(&error) {
                    return LinkOutcome::Outage(outage);
                }
                Err(error)
            }
        };
        self.store(library, &job, FeatureSource::Stream, measured)
            .await;
        LinkOutcome::Done
    }
}

impl LinksState {
    /// Waiting, unless a refusal or no yt-dlp says more.
    fn max_waiting(self) -> Self {
        match self {
            Self::Off => Self::Waiting,
            other => other,
        }
    }
}

/// The measurement itself, off the async threads.
async fn analyze(bytes: Vec<u8>) -> Result<mood::Features, String> {
    tauri::async_runtime::spawn_blocking(move || {
        mood::analyze(&mood::ffmpeg::samples(&bytes)).map_err(|err| err.to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_between_links_vary_within_range() {
        for _ in 0..50 {
            let pause = jitter((20, 60)).as_secs();
            assert!((20..=60).contains(&pause), "{pause}");
        }
    }

    #[test]
    fn statuses_say_what_settings_show() {
        let status = AnalysisStatus {
            state: AnalysisState::PowerSaving,
            links: LinksState::RateLimited,
            links_retry_at: Some(5),
            ..AnalysisStatus::default()
        };
        let json = serde_json::to_value(status).unwrap();
        assert_eq!(json["state"], "powerSaving");
        assert_eq!(json["links"], "rateLimited");
        assert_eq!(json["linksRetryAt"], 5);
        assert_eq!(json["filesDone"], 0);
    }
}
