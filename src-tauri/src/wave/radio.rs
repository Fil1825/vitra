//! When the library runs short for the wave's mood: songs like the playing track from
//! YouTube Music's radio of it (`RDAMVM<id>`). They join the library as links and are
//! measured before any other link, after which the wave can pick them; when nothing else is
//! left, they play before they are measured rather than the music stopping.

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use import::ImportedTrack;
use library::{Track, TrackFilter};
use tauri::Manager;

use super::{WaveService, lock, playable};
use crate::error::AppError;
use crate::import::ImportService;
use crate::links::LinkService;
use crate::player::Entry;
use crate::settings::SettingsStore;

/// Songs asked of one radio.
const SONGS: usize = 25;
/// At most one radio this often: one listing, then every song is measured (slowly).
const INTERVAL: Duration = Duration::from_secs(10 * 60);
/// The wave's last picks that may seed a radio when the playing track can't.
const SEEDS_BACK: usize = 5;

#[derive(Default)]
pub(super) struct Radio {
    running: AtomicBool,
    last: Mutex<Option<Instant>>,
    /// Videos whose radio was asked for this session.
    seeds: Mutex<HashSet<String>>,
    /// Library tracks already looked for on YouTube Music this session.
    searched: Mutex<HashSet<i64>>,
    /// The radio's songs, for when the wave has nothing measured left to pick.
    songs: Mutex<VecDeque<i64>>,
}

/// What a search on YouTube Music looks for, from a library track.
fn wanted(track: &Track) -> ImportedTrack {
    ImportedTrack {
        title: track.title.clone(),
        artists: if track.artists.is_empty() {
            track.artist.clone().into_iter().collect()
        } else {
            track.artists.iter().map(|a| a.name.clone()).collect()
        },
        album: track.album.clone(),
        duration: track.duration,
        ..ImportedTrack::default()
    }
}

impl WaveService {
    /// Whether a radio may be asked for now: it is on, its songs can join the library, links
    /// can be reached, and the last one was a while ago.
    fn radio_allowed(&self) -> bool {
        let settings = self.0.app.try_state::<SettingsStore>().map(|s| s.get());
        let on = settings
            .as_ref()
            .is_none_or(|s| s.wave.radio && s.wave.analyze_links && s.library.add_opened_files);
        let recent = lock(&self.0.radio.last).is_some_and(|at| at.elapsed() < INTERVAL);
        on && !recent && !self.0.analyzer.offline()
    }

    /// The library ran short: a radio, in the background, when allowed.
    pub(super) fn radio_soon(&self) {
        if !self.radio_allowed() || self.0.radio.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            match service.fetch_radio().await {
                Ok(0) => {}
                Ok(count) => log::info!("My Wave: {count} songs from YouTube Music's radio"),
                Err(err) => log::info!("My Wave: no radio: {err}"),
            }
            service.0.radio.running.store(false, Ordering::Release);
        });
    }

    async fn fetch_radio(&self) -> Result<usize, AppError> {
        // Every attempt counts, a failed search included: YouTube is asked at most this often
        // even while it can't be reached.
        *lock(&self.0.radio.last) = Some(Instant::now());
        let Some(seed) = self.radio_seed().await? else {
            return Ok(0);
        };
        lock(&self.0.radio.seeds).insert(seed.clone());
        let links = self
            .0
            .app
            .try_state::<LinkService>()
            .map(|l| l.inner().clone())
            .ok_or_else(|| AppError::Player("links are not available".into()))?;
        let tracks = links.radio(&seed, SONGS).await?;
        let ids: Vec<i64> = tracks
            .iter()
            .filter(|t| links::is_remote(&t.path))
            .map(|t| t.id)
            .collect();
        {
            let mut songs = lock(&self.0.radio.songs);
            songs.retain(|id| !ids.contains(id));
            songs.extend(&ids);
        }
        // Measured before any other link; each one measured makes it a candidate.
        self.0.analyzer.want(&ids);
        if self.0.starved.load(Ordering::Acquire) {
            self.top_up();
        }
        Ok(ids.len())
    }

    /// The video to ask the radio of: the playing track or one of the wave's last picks if
    /// it is a YouTube link, else YouTube Music's song for one of them (one search a radio).
    async fn radio_seed(&self) -> Result<Option<String>, AppError> {
        let playing = self.player()?.state().track.and_then(|t| t.library_id);
        let mut ids: Vec<i64> = playing.into_iter().collect();
        if let Some(wave) = lock(&self.0.wave).as_ref() {
            ids.extend(wave.picked().iter().rev().take(SEEDS_BACK));
        }
        let mut seen = HashSet::new();
        ids.retain(|id| seen.insert(*id));
        let tracks = self
            .library()?
            .run(move |db| db.tracks(&TrackFilter::Ids(ids)))
            .await?;
        let used = lock(&self.0.radio.seeds).clone();
        let fresh = |id: Option<String>| id.filter(|id| !used.contains(id));
        if let Some(id) = tracks
            .iter()
            .find_map(|t| fresh(links::youtube_id(&t.path)))
        {
            return Ok(Some(id));
        }
        let Some(track) = tracks
            .iter()
            .filter(|t| !links::is_remote(&t.path))
            .find(|t| lock(&self.0.radio.searched).insert(t.id))
        else {
            return Ok(None);
        };
        let Some(import) = self.0.app.try_state::<ImportService>() else {
            return Ok(None);
        };
        let found = import.find_on_youtube(&wanted(track)).await?;
        Ok(fresh(found.as_deref().and_then(links::youtube_id)))
    }

    /// Up to `count` of the radio's songs the wave hasn't played, as they are (measured or
    /// not), for when it has nothing else; they count as played in the wave.
    pub(super) async fn radio_songs(&self, count: usize) -> Result<Vec<Entry>, AppError> {
        let ids: Vec<i64> = {
            let wave = lock(&self.0.wave);
            let played: HashSet<i64> = wave
                .as_ref()
                .map(|w| w.picked().iter().copied().collect())
                .unwrap_or_default();
            let mut songs = lock(&self.0.radio.songs);
            songs.retain(|id| !played.contains(id));
            let take = count.min(songs.len());
            songs.drain(..take).collect()
        };
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let entries: Vec<Entry> = self
            .library()?
            .entries(ids)
            .await?
            .into_iter()
            .filter(playable)
            .collect();
        if let Some(wave) = lock(&self.0.wave).as_mut() {
            for id in entries.iter().filter_map(|e| e.track_id) {
                wave.played(id);
            }
        }
        Ok(entries)
    }
}
