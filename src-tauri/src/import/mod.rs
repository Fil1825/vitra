//! Playlist import (stage 9): whatever the user has, a link, a file, pasted text or (on macOS)
//! the Music app's playlists, becomes a library playlist at once. Tracks the library has
//! play right away; the others are looked for on YouTube Music in the background, one search
//! every few seconds, and light up as they are found. Doubtful matches keep alternatives for
//! the user to pick from. See `crates/import` for the reading and matching.

pub mod commands;
pub mod music;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use import::links::{self as service_links, Service, ServiceLink, Target};
use import::matching::{self, Candidate, Confidence};
use import::pages::{self, PageError};
use import::text::Item;
use import::{ImportedPlaylist, ImportedTrack};
use library::{EntryCandidate, EntryState, ImportEntry, ImportProgress, NewEntry, Playlist, Track};
use links::{LinkError, LinkTrack};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::error::AppError;
use crate::http;
use crate::library::{Change, LibraryService};
use crate::links::LinkService;

/// Sent while a playlist's tracks are being looked for.
pub const PROGRESS_EVENT: &str = "import://progress";

/// At least this long between two requests to YouTube, so a long playlist doesn't get the
/// app's searches blocked.
const SEARCH_INTERVAL: Duration = Duration::from_secs(3);
/// Results of a search looked at, and opened in full to compare artists and lengths.
const SEARCH_RESULTS: usize = 5;
const DESCRIBED: usize = 2;
/// Alternatives kept for the user to pick from.
const ALTERNATIVES: usize = 3;
/// After failures (no network, YouTube asking to sign in), the matcher waits this long.
const BACKOFF: Duration = Duration::from_secs(60);
/// Pages of Deezer's API read at most (a page is up to 400 tracks).
const DEEZER_PAGES: usize = 25;
/// Library tracks scored per entry.
const LIBRARY_CANDIDATES: usize = 30;

/// What an import made.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub playlist: Playlist,
    pub total: usize,
    /// Tracks found in the library right away.
    pub in_library: usize,
    /// The service gave only part of the list (copy the tracks for the rest).
    pub partial: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    playlist_id: i64,
    #[serde(flatten)]
    progress: ImportProgress,
}

struct Inner {
    app: AppHandle,
    wake: Notify,
    running: AtomicBool,
    last_request: Mutex<Option<Instant>>,
}

#[derive(Clone)]
pub struct ImportService(Arc<Inner>);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A library track as a candidate for matching.
fn library_candidate(track: &Track) -> Candidate {
    Candidate {
        title: track.title.clone(),
        artists: if track.artists.is_empty() {
            track.artist.clone().into_iter().collect()
        } else {
            track.artists.iter().map(|a| a.name.clone()).collect()
        },
        album: track.album.clone(),
        duration: track.duration,
        isrc: None,
    }
}

fn link_candidate(track: &LinkTrack) -> Candidate {
    Candidate {
        title: track.title.clone(),
        artists: if track.artists.is_empty() {
            track.artist.clone().into_iter().collect()
        } else {
            track.artists.clone()
        },
        album: track.album.clone(),
        duration: track.duration,
        isrc: None,
    }
}

fn entry_candidate(track: &LinkTrack, score: f64) -> EntryCandidate {
    let candidate = link_candidate(track);
    EntryCandidate {
        track_id: None,
        url: Some(track.url.clone()),
        title: candidate.title,
        artists: candidate.artists,
        album: candidate.album,
        duration: candidate.duration,
        score,
    }
}

/// The best library track for `wanted`, with its score.
fn best_in_library(
    db: &library::Library,
    wanted: &ImportedTrack,
) -> library::Result<Option<(Track, f64)>> {
    let tracks = db.library_candidates(wanted, LIBRARY_CANDIDATES)?;
    if tracks.len() == 1 && wanted.path.as_deref() == Some(tracks[0].path.as_str()) {
        return Ok(tracks.into_iter().next().map(|track| (track, 1.0)));
    }
    let candidates: Vec<Candidate> = tracks.iter().map(library_candidate).collect();
    Ok(matching::rank(wanted, &candidates)
        .into_iter()
        .next()
        .map(|(index, score)| (tracks[index].clone(), score)))
}

/// Entries for a list, matched against the library now: sure matches play at once, the rest
/// wait for the background search.
fn match_in_library(
    db: &library::Library,
    tracks: Vec<ImportedTrack>,
) -> library::Result<Vec<NewEntry>> {
    tracks
        .into_iter()
        .map(|source| {
            if source.title.is_empty() {
                return Ok(NewEntry::pending(source));
            }
            match best_in_library(db, &source)? {
                Some((track, score)) if Confidence::of(score) == Confidence::Sure => Ok(NewEntry {
                    track_id: Some(track.id),
                    state: EntryState::Sure,
                    ..NewEntry::pending(source)
                }),
                _ => Ok(NewEntry::pending(source)),
            }
        })
        .collect()
}

impl ImportService {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(Inner {
            app,
            wake: Notify::new(),
            running: AtomicBool::new(false),
            last_request: Mutex::new(None),
        }))
    }

    fn library(&self) -> Result<LibraryService, AppError> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|library| library.inner().clone())
            .ok_or_else(|| AppError::Player("the library is not available".into()))
    }

    fn links(&self) -> Result<LinkService, AppError> {
        self.0
            .app
            .try_state::<LinkService>()
            .map(|links| links.inner().clone())
            .ok_or_else(|| AppError::Player("links are not available".into()))
    }

    /// Starts the background matcher (once).
    pub fn start(&self) {
        if self.0.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move { service.matcher().await });
    }

    fn wake(&self) {
        self.0.wake.notify_one();
    }

    /// Makes the playlist (or brings the one imported from `source_url` up to date).
    async fn save(
        &self,
        name: String,
        source_url: Option<String>,
        playlist: ImportedPlaylist,
    ) -> Result<ImportResult, AppError> {
        let library = self.library()?;
        let total = playlist.tracks.len();
        let partial = playlist.partial;
        let tracks = playlist.tracks;
        let (playlist, in_library) = library
            .run(move |db| {
                let entries = match_in_library(db, tracks)?;
                let in_library = entries.iter().filter(|e| e.track_id.is_some()).count();
                let existing = match &source_url {
                    Some(url) => db.imported_playlist(url)?,
                    None => None,
                };
                let playlist = match existing {
                    Some(id) => {
                        db.reimport_playlist(id, &entries)?;
                        db.playlist(id)?
                            .ok_or_else(|| library::Error::Invalid("playlist vanished".into()))?
                    }
                    None => db.create_imported_playlist(&name, source_url.as_deref(), &entries)?,
                };
                Ok((playlist, in_library))
            })
            .await?;
        library.changed(Change::Playlists);
        self.emit_progress(playlist.id).await;
        self.wake();
        Ok(ImportResult {
            playlist,
            total,
            in_library,
            partial,
        })
    }

    /// Pasted text: links (one list, or tracks in bulk) or tracks written out.
    pub async fn import_text(&self, text: &str, name: &str) -> Result<ImportResult, AppError> {
        let items = import::text::parse(text);
        if items.is_empty() {
            return Err(AppError::Import("no tracks in the text".into()));
        }
        // One link alone: the list it points at.
        if let [Item::Link(url)] = items.as_slice()
            && let Some(link) = service_links::parse(url)
            && !matches!(link.target, Target::Track { .. })
        {
            return self.import_link(url).await;
        }
        let tracks = items
            .into_iter()
            .map(|item| match item {
                Item::Track(track) => track,
                // Read when its turn comes (see `describe_entry`).
                Item::Link(url) => ImportedTrack {
                    url: Some(service_links::parse(&url).map_or(url, |link| link.url)),
                    ..ImportedTrack::default()
                },
            })
            .collect();
        self.save(
            name.to_owned(),
            None,
            ImportedPlaylist {
                tracks,
                ..ImportedPlaylist::default()
            },
        )
        .await
    }

    /// A playlist file (the Music app's XML may hold several).
    pub async fn import_file(&self, path: &Path) -> Result<Vec<ImportResult>, AppError> {
        let bytes = tokio::fs::read(path).await?;
        let path_owned = path.to_path_buf();
        let playlists =
            tauri::async_runtime::spawn_blocking(move || import::files::read(&path_owned, &bytes))
                .await
                .map_err(|err| AppError::Task(err.to_string()))?
                .map_err(|err| AppError::Import(err.to_string()))?;
        let mut results = Vec::new();
        for playlist in playlists {
            let name = playlist.name.clone().unwrap_or_default();
            results.push(self.save(name, None, playlist).await?);
        }
        Ok(results)
    }

    /// A link to a list on a service (or a playable one, which links import as before).
    pub async fn import_link(&self, url: &str) -> Result<ImportResult, AppError> {
        if let Some(id) = url.strip_prefix(music::SOURCE_PREFIX) {
            return self
                .import_music(vec![id.to_owned()])
                .await?
                .pop()
                .ok_or_else(|| {
                    AppError::Import("the playlist is no longer in the Music app".into())
                });
        }
        let mut link = service_links::parse(url)
            .ok_or_else(|| AppError::Import("not a link to a playlist".into()))?;
        if link.target == Target::Short {
            let target = http::follow(&link.url).await?;
            link = service_links::parse(&target).ok_or_else(|| {
                AppError::Import("the short link leads nowhere Vitra knows".into())
            })?;
        }
        match link.service {
            Service::Playable => {
                let playlist = self.links()?.import(&link.url).await?;
                let total = playlist.track_count.max(0) as usize;
                return Ok(ImportResult {
                    playlist,
                    total,
                    in_library: 0,
                    partial: false,
                });
            }
            Service::Vk => {
                return Err(AppError::Import(
                    "VK shows its playlists only to signed-in visitors: open the playlist in \
                     your browser, select all (⌘A or Ctrl+A), copy and paste here"
                        .into(),
                ));
            }
            _ => {}
        }
        let playlist = self.read_link(&link).await?;
        let name = playlist.name.clone().unwrap_or_default();
        self.save(name, Some(link.url.clone()), playlist).await
    }

    /// Playlists of the Music app (macOS), each kept as the playlist imported from it.
    pub async fn import_music(&self, ids: Vec<String>) -> Result<Vec<ImportResult>, AppError> {
        let mut results = Vec::new();
        for (id, playlist) in music::read(ids).await? {
            let name = playlist.name.clone().unwrap_or_default();
            results.push(
                self.save(name, Some(music::source_url(&id)), playlist)
                    .await?,
            );
        }
        Ok(results)
    }

    /// Reads a service's page for `link` (following Deezer's pages).
    async fn read_link(&self, link: &ServiceLink) -> Result<ImportedPlaylist, AppError> {
        let request = pages::request(link)
            .ok_or_else(|| AppError::Import("Vitra can't read lists from this link".into()))?;
        let response = http::get(&request.url, &request.headers).await?;
        if response.status == 404 {
            return Err(page_error(PageError::NotFound));
        }
        if link.service == Service::Deezer {
            let mut page = pages::deezer(&response.body).map_err(page_error)?;
            let mut playlist = std::mem::take(&mut page.playlist);
            let mut next = page.next;
            for _ in 0..DEEZER_PAGES {
                let Some(url) = next.take() else {
                    break;
                };
                let response = http::get(&url, &[]).await?;
                let page = pages::deezer(&response.body).map_err(page_error)?;
                playlist.tracks.extend(page.playlist.tracks);
                next = page.next;
            }
            playlist.partial = next.is_some();
            if playlist.tracks.is_empty() {
                return Err(page_error(PageError::Unreadable("no tracks".into())));
            }
            return Ok(playlist);
        }
        pages::read(link, &response.body).map_err(page_error)
    }

    /// The import entries of a playlist's page.
    pub async fn places(&self, playlist_id: i64) -> Result<Vec<library::PlaylistPlace>, AppError> {
        self.library()?
            .run(move |db| db.playlist_places(playlist_id))
            .await
    }

    pub async fn progress(&self, playlist_id: i64) -> Result<ImportProgress, AppError> {
        self.library()?
            .run(move |db| db.import_progress(playlist_id))
            .await
    }

    async fn emit_progress(&self, playlist_id: i64) {
        if let Ok(progress) = self.progress(playlist_id).await
            && let Err(err) = self.0.app.emit(
                PROGRESS_EVENT,
                Progress {
                    playlist_id,
                    progress,
                },
            )
        {
            log::warn!("cannot emit {PROGRESS_EVENT}: {err}");
        }
    }

    /// The user picked a track for an entry (an alternative, or one they found).
    pub async fn pick(&self, entry_id: i64, candidate: EntryCandidate) -> Result<(), AppError> {
        let library = self.library()?;
        let entry = library
            .run(move |db| db.import_entry(entry_id))
            .await?
            .ok_or_else(|| AppError::NotFound(format!("import entry {entry_id}")))?;
        let track_id = match (candidate.track_id, &candidate.url) {
            (Some(id), _) => id,
            (None, Some(url)) => {
                let url = url.clone();
                let source = entry.source.clone();
                library
                    .run(move |db| db.add_found_link(&url, &source))
                    .await?
            }
            (None, None) => return Err(AppError::Import("nothing to pick".into())),
        };
        let mut candidates = entry.candidates.clone();
        // The picked one leads, so it shows as the choice.
        candidates.retain(|c| c.url != candidate.url || c.track_id != candidate.track_id);
        candidates.insert(0, candidate);
        library
            .run(move |db| {
                db.resolve_entry(entry_id, EntryState::Picked, Some(track_id), &candidates)
            })
            .await?;
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        self.emit_progress(entry.playlist_id).await;
        Ok(())
    }

    /// Tracks that might be an entry, for "Find…": the library's, then YouTube Music's for
    /// `query` (the entry's names when empty).
    pub async fn search(
        &self,
        entry_id: i64,
        query: Option<String>,
    ) -> Result<Vec<EntryCandidate>, AppError> {
        let library = self.library()?;
        let entry = library
            .run(move |db| db.import_entry(entry_id))
            .await?
            .ok_or_else(|| AppError::NotFound(format!("import entry {entry_id}")))?;
        let mut wanted = entry.source.clone();
        if let Some(query) = query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
            wanted = ImportedTrack {
                swappable: true,
                ..split_query(query)
            };
        }
        let lookup = wanted.clone();
        let mut found: Vec<EntryCandidate> = library
            .run(move |db| {
                let tracks = db.library_candidates(&lookup, LIBRARY_CANDIDATES)?;
                let candidates: Vec<Candidate> = tracks.iter().map(library_candidate).collect();
                Ok(matching::rank(&lookup, &candidates)
                    .into_iter()
                    .take(ALTERNATIVES)
                    .filter(|(_, score)| *score > 0.3)
                    .map(|(index, score)| {
                        let c = &candidates[index];
                        EntryCandidate {
                            track_id: Some(tracks[index].id),
                            url: None,
                            title: c.title.clone(),
                            artists: c.artists.clone(),
                            album: c.album.clone(),
                            duration: c.duration,
                            score,
                        }
                    })
                    .collect())
            })
            .await?;
        let online = self.search_online(&wanted, query.as_deref()).await?;
        found.extend(
            online
                .into_iter()
                .map(|(track, score)| entry_candidate(&track, score)),
        );
        Ok(found)
    }

    /// Looks again for the entries that weren't found.
    pub async fn retry(&self, playlist_id: i64) -> Result<(), AppError> {
        self.library()?
            .run(move |db| db.retry_entries(playlist_id, true))
            .await?;
        self.emit_progress(playlist_id).await;
        self.wake();
        Ok(())
    }

    /// Waits until YouTube may be asked again.
    async fn pace(&self) {
        let wait = lock(&self.0.last_request)
            .map(|at| SEARCH_INTERVAL.saturating_sub(at.elapsed()))
            .unwrap_or_default();
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
        *lock(&self.0.last_request) = Some(Instant::now());
    }

    /// YouTube Music's songs for `wanted`, the closest opened in full and scored, best first.
    async fn search_online(
        &self,
        wanted: &ImportedTrack,
        query: Option<&str>,
    ) -> Result<Vec<(LinkTrack, f64)>, AppError> {
        let ytdlp = self.links()?.ytdlp()?;
        let query = query
            .map(str::to_owned)
            .unwrap_or_else(|| matching::search_query(wanted));
        self.pace().await;
        let results = ytdlp.search(&query, SEARCH_RESULTS).await?;
        // The listing has titles only: open the ones whose titles come close.
        let mut by_title: Vec<(usize, f64)> = results
            .iter()
            .enumerate()
            .map(|(index, track)| {
                let candidate = Candidate {
                    title: track.title.clone(),
                    ..Candidate::default()
                };
                let title_only = ImportedTrack {
                    artists: Vec::new(),
                    duration: None,
                    ..wanted.clone()
                };
                (index, matching::score(&title_only, &candidate))
            })
            .collect();
        by_title.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut scored = Vec::new();
        for (index, _) in by_title.into_iter().take(DESCRIBED) {
            self.pace().await;
            let track = match ytdlp.describe(&results[index].url).await {
                Ok(track) => track,
                Err(err) => {
                    log::info!("import: cannot open {}: {err}", results[index].url);
                    continue;
                }
            };
            let score = matching::score(wanted, &link_candidate(&track));
            scored.push((track, score));
            // A sure first one is enough.
            if Confidence::of(score) == Confidence::Sure {
                break;
            }
        }
        // The rest stay as titles, for the user to try.
        for (index, track) in results.iter().enumerate() {
            if !scored.iter().any(|(t, _)| t.url == track.url) && index < SEARCH_RESULTS {
                let score = matching::score(wanted, &link_candidate(track)) * 0.5;
                scored.push((track.clone(), score));
            }
        }
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        Ok(scored)
    }

    /// Reads a bare link entry's page (Spotify tracks pasted in bulk) for its names.
    async fn describe_entry(&self, entry: &ImportEntry) -> Result<Option<ImportedTrack>, AppError> {
        let Some(url) = &entry.source.url else {
            return Ok(None);
        };
        let Some(link) = service_links::parse(url) else {
            return Ok(None);
        };
        if link.service == Service::Playable {
            // A YouTube or SoundCloud link is a track already: yt-dlp names it.
            self.pace().await;
            let track = self.links()?.ytdlp()?.describe(&link.url).await?;
            return Ok(Some(ImportedTrack {
                title: track.title.clone(),
                artists: link_candidate(&track).artists,
                album: track.album.clone(),
                duration: track.duration,
                url: Some(track.url),
                ..ImportedTrack::default()
            }));
        }
        let playlist = self.read_link(&link).await?;
        Ok(playlist
            .tracks
            .into_iter()
            .next()
            .map(|track| ImportedTrack {
                url: Some(link.url.clone()),
                ..track
            }))
    }

    /// The background matcher: entries waiting are looked for one at a time.
    async fn matcher(self) {
        let mut failures = 0u32;
        loop {
            let pending = match self.library() {
                Ok(library) => library.run(|db| db.pending_entries(8)).await,
                Err(err) => Err(err),
            };
            let pending = match pending {
                Ok(pending) => pending,
                Err(err) => {
                    log::warn!("import: {err}");
                    tokio::time::sleep(BACKOFF).await;
                    continue;
                }
            };
            if pending.is_empty() {
                self.0.wake.notified().await;
                continue;
            }
            for entry in pending {
                match self.find(&entry).await {
                    Ok(()) => failures = 0,
                    Err(err) => {
                        failures += 1;
                        log::info!("import: looking for {:?} failed: {err}", entry.source.title);
                        // yt-dlp missing, no network, YouTube asking to sign in: give it time.
                        let wait = BACKOFF * failures.min(10);
                        tokio::time::sleep(wait).await;
                        break;
                    }
                }
            }
        }
    }

    /// Looks for one entry: the library first (it may have the track by now), then
    /// YouTube Music.
    async fn find(&self, entry: &ImportEntry) -> Result<(), AppError> {
        let library = self.library()?;
        let mut source = entry.source.clone();
        if source.title.is_empty() {
            match self.describe_entry(entry).await {
                Ok(Some(described)) => {
                    let id = entry.id;
                    let stored = described.clone();
                    library
                        .run(move |db| db.update_entry_source(id, &stored))
                        .await?;
                    source = described;
                }
                Ok(None) | Err(_) => {
                    let id = entry.id;
                    library
                        .run(move |db| db.resolve_entry(id, EntryState::Missing, None, &[]))
                        .await?;
                    self.emit_progress(entry.playlist_id).await;
                    return Ok(());
                }
            }
        }
        // A link that is a track already plays as it is.
        if let Some(url) = &source.url
            && service_links::parse(url).is_some_and(|l| l.service == Service::Playable)
        {
            let (id, url, stored) = (entry.id, url.clone(), source.clone());
            let track = library
                .run(move |db| db.add_found_link(&url, &stored))
                .await?;
            library
                .run(move |db| db.resolve_entry(id, EntryState::Sure, Some(track), &[]))
                .await?;
            return self.announce(entry.playlist_id).await;
        }
        let wanted = source.clone();
        let local = library.run(move |db| best_in_library(db, &wanted)).await?;
        if let Some((track, score)) = &local
            && Confidence::of(*score) == Confidence::Sure
        {
            let (id, track_id) = (entry.id, track.id);
            library
                .run(move |db| db.resolve_entry(id, EntryState::Sure, Some(track_id), &[]))
                .await?;
            return self.announce(entry.playlist_id).await;
        }
        let found = match self.search_online(&source, None).await {
            Ok(found) => found,
            Err(AppError::Link(LinkError::Failed(message))) if message.contains("nothing") => {
                Vec::new()
            }
            Err(err) => return Err(err),
        };
        let mut candidates: Vec<EntryCandidate> = found
            .iter()
            .map(|(track, score)| entry_candidate(track, *score))
            .collect();
        // A doubtful library track is an alternative too.
        if let Some((track, score)) = &local
            && Confidence::of(*score) == Confidence::Doubtful
        {
            let c = library_candidate(track);
            candidates.push(EntryCandidate {
                track_id: Some(track.id),
                url: None,
                title: c.title,
                artists: c.artists,
                album: c.album,
                duration: c.duration,
                score: *score,
            });
            candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        }
        candidates.truncate(ALTERNATIVES + 1);
        let best = candidates
            .first()
            .filter(|best| Confidence::of(best.score) > Confidence::None)
            .cloned();
        let id = entry.id;
        // A library track, or a link stored under the import's names.
        let track_id = match best.as_ref().map(|b| (b.track_id, b.url.clone())) {
            Some((Some(track_id), _)) => Some(track_id),
            Some((None, Some(url))) => {
                let stored = source.clone();
                Some(
                    library
                        .run(move |db| db.add_found_link(&url, &stored))
                        .await?,
                )
            }
            _ => None,
        };
        let state = match (track_id, best.map(|b| Confidence::of(b.score))) {
            (Some(_), Some(Confidence::Sure)) => EntryState::Sure,
            (Some(_), _) => EntryState::Doubtful,
            (None, _) => EntryState::Missing,
        };
        library
            .run(move |db| db.resolve_entry(id, state, track_id, &candidates))
            .await?;
        self.announce(entry.playlist_id).await
    }

    async fn announce(&self, playlist_id: i64) -> Result<(), AppError> {
        let library = self.library()?;
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        self.emit_progress(playlist_id).await;
        Ok(())
    }
}

/// "Artist - Title" typed into Find…, either way round.
fn split_query(query: &str) -> ImportedTrack {
    match import::text::parse(query).into_iter().next() {
        Some(Item::Track(track)) => track,
        _ => ImportedTrack::new(query, Vec::new()),
    }
}

/// What to tell the user when a page gave no list.
fn page_error(err: PageError) -> AppError {
    AppError::Import(match err {
        PageError::NotFound => "the service has no such playlist (is the link complete?)".into(),
        PageError::Private => {
            "the playlist is private: make it public, or copy its tracks and paste them here".into()
        }
        PageError::Unreadable(detail) => format!(
            "this link can't be read right now ({detail}); copy the playlist's tracks and paste them instead"
        ),
    })
}
