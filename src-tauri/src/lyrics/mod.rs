//! Lyrics of the playing track: found by priority (see [`sources`]), looked up on LRCLIB when
//! allowed, followed line by line during playback, and saved by the editor.
//!
//! The player tells the service when a track starts and once its tags are read; the service
//! emits `lyrics://changed` with the whole [`CurrentLyrics`] and `lyrics://line` when the
//! playing line changes. The lyrics view keeps its own clock for smooth karaoke, so the line
//! event is for everything else (the mini player, plugins).

mod backup;
mod commands;
mod online;
mod sources;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use library::{SOURCE_ONLINE, SOURCE_USER};
use lyrics::Lyrics;
use lyrics::lrclib::Query;
use media::TrackMetadata;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::library::LibraryService;
use crate::settings::SettingsStore;

pub use commands::*;
use online::Answer;
pub use sources::Source;
use sources::{Chosen, Found};

pub const CHANGED_EVENT: &str = "lyrics://changed";
pub const LINE_EVENT: &str = "lyrics://line";

/// An online search that found nothing is repeated after this long (seconds), in case the
/// lyrics were added since.
const RECHECK_AFTER: i64 = 30 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Nothing is playing.
    #[default]
    Idle,
    Loading,
    /// Looking the track up online.
    Searching,
    Ready,
    /// No lyrics anywhere we looked.
    Missing,
}

/// The playing track's lyrics.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentLyrics {
    /// The player's track id (`player://state`) these belong to.
    pub track_id: Option<u64>,
    pub path: Option<String>,
    pub library_id: Option<i64>,
    pub status: Status,
    /// Timestamps are positions in the track: the file's own `[offset]` is applied.
    pub lyrics: Option<Lyrics>,
    pub source: Option<Source>,
    /// The user's sync adjustment in milliseconds; positive shows the lyrics later.
    pub offset_ms: i64,
    /// Why the last online search failed (it is tried again with the next play).
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LineUpdate {
    track_id: u64,
    index: Option<usize>,
}

/// What the player knows about the playing track.
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub id: u64,
    pub path: String,
    pub library_id: Option<i64>,
    pub title: String,
    pub metadata: TrackMetadata,
}

/// Where the editor saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Target {
    /// The file's tags (a backup is taken first).
    Tags,
    /// A `.lrc` next to the file.
    File,
    /// Only the app's library.
    Library,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lookup {
    /// Online when the setting allows and no recent search came back empty.
    Auto,
    /// The user asked: search now.
    Now,
}

#[derive(Debug, Default)]
struct Line {
    track_id: Option<u64>,
    /// Seconds.
    position: f64,
    index: Option<usize>,
}

#[derive(Clone)]
pub struct LyricsService(Arc<Inner>);

struct Inner {
    app: AppHandle,
    backups: PathBuf,
    current: Mutex<CurrentLyrics>,
    /// The playing track as the player last described it.
    track: Mutex<Option<TrackInfo>>,
    line: Mutex<Line>,
    /// Sync offsets of tracks outside the library, for this session.
    offsets: Mutex<HashMap<String, i64>>,
    /// Tracks outside the library that LRCLIB had nothing for, this session.
    misses: Mutex<HashSet<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn is_local(path: &str) -> bool {
    !links::is_remote(path)
}

impl LyricsService {
    pub fn new(app: AppHandle, data_dir: &Path) -> Self {
        Self(Arc::new(Inner {
            app,
            backups: data_dir.join("backups"),
            current: Mutex::new(CurrentLyrics::default()),
            track: Mutex::new(None),
            line: Mutex::new(Line::default()),
            offsets: Mutex::new(HashMap::new()),
            misses: Mutex::new(HashSet::new()),
        }))
    }

    pub fn current(&self) -> CurrentLyrics {
        lock(&self.0.current).clone()
    }

    fn library(&self) -> Option<LibraryService> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|library| library.inner().clone())
    }

    fn online_allowed(&self) -> bool {
        self.0
            .app
            .try_state::<SettingsStore>()
            .is_some_and(|settings| settings.get().lyrics.online)
    }

    /// A new track started: its lyrics are loaded once its tags are read
    /// ([`Self::track_ready`]).
    pub fn track_started(&self, id: u64, path: &str, library_id: Option<i64>) {
        let offset_ms = match library_id {
            Some(_) => 0,
            None => lock(&self.0.offsets).get(path).copied().unwrap_or(0),
        };
        *lock(&self.0.track) = None;
        *lock(&self.0.current) = CurrentLyrics {
            track_id: Some(id),
            path: Some(path.to_owned()),
            library_id,
            status: Status::Loading,
            offset_ms,
            ..CurrentLyrics::default()
        };
        *lock(&self.0.line) = Line {
            track_id: Some(id),
            ..Line::default()
        };
        self.emit_changed();
    }

    /// The playing track's tags were read (again, after an edit): load its lyrics.
    pub fn track_ready(&self, track: TrackInfo) {
        if lock(&self.0.current).track_id != Some(track.id) {
            return;
        }
        *lock(&self.0.track) = Some(track.clone());
        self.spawn_load(track, Lookup::Auto);
    }

    /// The playing track joined or left the library: its saved lyrics and offset count now.
    pub fn library_id_changed(&self, id: u64, library_id: Option<i64>) {
        {
            let mut current = lock(&self.0.current);
            if current.track_id != Some(id) {
                return;
            }
            current.library_id = library_id;
        }
        if let Some(track) = lock(&self.0.track).as_mut().filter(|t| t.id == id) {
            track.library_id = library_id;
        }
        self.reload();
    }

    /// Loads the playing track's lyrics again (after an edit or a settings change).
    pub fn reload(&self) {
        if let Some(track) = lock(&self.0.track).clone() {
            self.spawn_load(track, Lookup::Auto);
        }
    }

    /// Searches LRCLIB for the playing track now, whatever the settings say.
    pub fn search_online(&self) {
        if let Some(track) = lock(&self.0.track).clone() {
            self.spawn_load(track, Lookup::Now);
        }
    }

    fn spawn_load(&self, track: TrackInfo, lookup: Lookup) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move { service.load(track, lookup).await });
    }

    /// Changes the current lyrics if they still belong to track `id`; returns whether they
    /// did.
    fn update(&self, id: u64, change: impl FnOnce(&mut CurrentLyrics)) -> bool {
        {
            let mut current = lock(&self.0.current);
            if current.track_id != Some(id) {
                return false;
            }
            change(&mut current);
        }
        self.emit_changed();
        let position = lock(&self.0.line).position;
        self.position(id, position);
        true
    }

    async fn load(&self, track: TrackInfo, lookup: Lookup) {
        let library = self.library();
        let (path, library_id) = (track.path.clone(), track.library_id);
        let local = tauri::async_runtime::spawn_blocking(move || {
            let mut found = Found::default();
            let mut offset = None;
            let mut checked_at = None;
            if let (Some(library), Some(id)) = (&library, library_id) {
                let stored = library.with(|db| {
                    found.saved = db.saved_lyrics(id)?;
                    offset = Some(db.lyrics_offset(id)?);
                    checked_at = db.lyrics_lookup_at(id)?;
                    Ok(())
                });
                if let Err(err) = stored {
                    log::warn!("lyrics of track {id} in the library: {err}");
                }
            }
            if is_local(&path) {
                sources::read_local(Path::new(&path), &mut found);
            }
            let chosen = sources::choose(&found);
            // Words from a `.lrc` or a `SYLT` frame aren't in the index the scanner built.
            if let (Some(library), Some(id), Some(chosen)) = (&library, library_id, &chosen)
                && matches!(chosen.source, Source::File | Source::Tags)
                && let Err(err) = library.with(|db| db.index_lyrics(id, Some(&chosen.text)))
            {
                log::info!("cannot index the lyrics of track {id}: {err}");
            }
            (chosen, offset, checked_at)
        })
        .await;
        let Ok((chosen, offset, checked_at)) = local else {
            return;
        };

        let found_locally = chosen.is_some();
        let shown = self.update(track.id, |current| {
            if let Some(offset) = offset {
                current.offset_ms = offset;
            }
            current.error = None;
            show(current, chosen);
        });
        if !shown || (found_locally && lookup == Lookup::Auto) {
            return;
        }
        let wanted = match lookup {
            Lookup::Now => true,
            Lookup::Auto => {
                let recently_missed = match track.library_id {
                    Some(_) => checked_at.is_some_and(|at| library::now() - at < RECHECK_AFTER),
                    None => lock(&self.0.misses).contains(&track.path),
                };
                // Matching relies on the duration; a stream without one (radio) isn't a song.
                let song = track.metadata.duration.is_some_and(|d| d > 0.0);
                self.online_allowed() && song && !recently_missed
            }
        };
        if wanted {
            self.look_up(&track).await;
        }
    }

    async fn look_up(&self, track: &TrackInfo) {
        let metadata = &track.metadata;
        let query = Query {
            title: metadata
                .title
                .clone()
                .unwrap_or_else(|| track.title.clone()),
            artist: metadata.artist.clone(),
            album: metadata.album.clone(),
            duration: metadata.duration,
        };
        if !self.update(track.id, |current| current.status = Status::Searching) {
            return;
        }
        let answer = online::look_up(&query).await;
        let library = self.library();
        let id = track.library_id;
        let stored = match &answer {
            Ok(Answer::Found(text)) => {
                let text = text.clone();
                store(library, id, move |db, id| {
                    db.save_lyrics(id, &text, SOURCE_ONLINE)
                })
                .await
            }
            Ok(Answer::NotFound | Answer::Instrumental) => {
                if id.is_none() {
                    lock(&self.0.misses).insert(track.path.clone());
                }
                let now = library::now();
                store(library, id, move |db, id| db.note_lyrics_lookup(id, now)).await
            }
            Err(_) => Ok(()),
        };
        if let Err(err) = stored {
            log::warn!("cannot keep the online lyrics: {err}");
        }
        self.update(track.id, |current| match answer {
            Ok(Answer::Found(text)) => show(
                current,
                Some(Chosen {
                    lyrics: lyrics::parse(&text),
                    text,
                    source: Source::Online,
                }),
            ),
            Ok(_) => current.status = Status::Missing,
            Err(err) => {
                log::info!("LRCLIB: {err}");
                current.status = Status::Missing;
                current.error = Some(err.to_string());
            }
        });
    }

    /// Follows playback: emits `lyrics://line` when the playing line changes.
    pub fn position(&self, track_id: u64, seconds: f64) {
        let index = {
            let current = lock(&self.0.current);
            match (&current.lyrics, current.track_id == Some(track_id)) {
                (Some(lyrics), true) => {
                    let ms = (seconds * 1000.0) as i64 - current.offset_ms;
                    u64::try_from(ms).ok().and_then(|ms| lyrics.line_at(ms))
                }
                _ => None,
            }
        };
        let changed = {
            let mut line = lock(&self.0.line);
            let changed = line.track_id != Some(track_id) || line.index != index;
            *line = Line {
                track_id: Some(track_id),
                position: seconds,
                index,
            };
            changed
        };
        if changed {
            self.emit(LINE_EVENT, &LineUpdate { track_id, index });
        }
    }

    /// Sets the playing track's sync offset (kept in the library for library tracks).
    pub async fn set_offset(&self, offset_ms: i64) -> Result<(), AppError> {
        let offset_ms = offset_ms.clamp(-60_000, 60_000);
        let (id, path, library_id) = {
            let current = lock(&self.0.current);
            match (current.track_id, current.path.clone()) {
                (Some(id), Some(path)) => (id, path, current.library_id),
                _ => return Ok(()),
            }
        };
        match (library_id, self.library()) {
            (Some(track), Some(library)) => {
                library
                    .run(move |db| db.set_lyrics_offset(track, offset_ms))
                    .await?;
            }
            _ => {
                lock(&self.0.offsets).insert(path, offset_ms);
            }
        }
        self.update(id, |current| current.offset_ms = offset_ms);
        Ok(())
    }

    /// Saves lyrics edited for any track. Empty text removes them from the target.
    pub async fn save(
        &self,
        path: String,
        library_id: Option<i64>,
        text: String,
        target: Target,
    ) -> Result<(), AppError> {
        let library = self.library();
        let backups = self.0.backups.clone();
        let key = path.clone();
        tauri::async_runtime::spawn_blocking(move || {
            save_blocking(library.as_ref(), &backups, &key, library_id, &text, target)
        })
        .await??;
        if lock(&self.0.current).path.as_deref() == Some(path.as_str()) {
            self.reload();
        }
        Ok(())
    }

    fn emit_changed(&self) {
        let current = self.current();
        self.emit(CHANGED_EVENT, &current);
    }

    fn emit<S: Serialize + Clone>(&self, event: &str, payload: &S) {
        if let Err(err) = self.0.app.emit(event, payload) {
            log::warn!("cannot emit {event}: {err}");
        }
    }
}

fn show(current: &mut CurrentLyrics, chosen: Option<Chosen>) {
    match chosen {
        Some(chosen) => {
            current.status = Status::Ready;
            current.lyrics = Some(chosen.lyrics.normalized());
            current.source = Some(chosen.source);
        }
        None => {
            current.status = Status::Missing;
            current.lyrics = None;
            current.source = None;
        }
    }
}

/// Runs `f` for a library track, if there is one.
async fn store(
    library: Option<LibraryService>,
    track_id: Option<i64>,
    f: impl FnOnce(&mut library::Library, i64) -> library::Result<()> + Send + 'static,
) -> Result<(), AppError> {
    match (library, track_id) {
        (Some(library), Some(id)) => library.run(move |db| f(db, id)).await,
        _ => Ok(()),
    }
}

fn needs_library(
    library: Option<&LibraryService>,
    id: Option<i64>,
) -> Result<(&LibraryService, i64), AppError> {
    library
        .zip(id)
        .ok_or_else(|| AppError::Lyrics("the track is not in the library".into()))
}

fn save_blocking(
    library: Option<&LibraryService>,
    backups: &Path,
    path: &str,
    library_id: Option<i64>,
    text: &str,
    target: Target,
) -> Result<(), AppError> {
    let text = text.trim();
    if target != Target::Library && !is_local(path) {
        return Err(AppError::Lyrics("a link has no file to save into".into()));
    }
    let file = Path::new(path);
    match target {
        Target::Library => {
            let (library, id) = needs_library(library, library_id)?;
            library.with(|db| {
                if text.is_empty() {
                    db.delete_lyrics(id)?;
                    db.index_lyrics(id, None)
                } else {
                    db.save_lyrics(id, text, SOURCE_USER)
                }
            })?;
        }
        Target::Tags => {
            backup::back_up(file, &backups.join("tags"))?;
            media::write_lyrics(file, text)?;
        }
        Target::File => {
            let lrc = lyrics::lrc_path(file);
            if lrc.exists() {
                backup::back_up(&lrc, &backups.join("lrc"))?;
            }
            if text.is_empty() {
                if lrc.exists() {
                    std::fs::remove_file(&lrc)?;
                }
            } else {
                std::fs::write(&lrc, format!("{text}\n"))?;
            }
        }
    }
    // The file now holds what the user wants to see: lyrics saved in the app only would
    // hide it.
    if target != Target::Library
        && let (Some(library), Some(id)) = (library, library_id)
    {
        library.with(|db| {
            if !text.is_empty() {
                db.delete_lyrics(id)?;
            }
            db.index_lyrics(id, (!text.is_empty()).then_some(text))
        })?;
    }
    Ok(())
}
