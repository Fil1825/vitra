//! Playlist import (stage 9): whatever the user has, a link, a file, pasted text or (on macOS)
//! the Music app's playlists, becomes a library playlist at once. Tracks the library has
//! play right away; the others are looked for on YouTube Music in the background, one search
//! every few seconds, and light up as they are found. Doubtful matches keep alternatives for
//! the user to pick from. See `crates/import` for the reading and matching.

pub mod commands;
pub mod music;
pub mod status;

use std::collections::HashMap;
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
use links::{LinkError, LinkTrack, YtDlp};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::error::AppError;
use crate::http;
use crate::library::{Change, LibraryService};
use crate::links::LinkService;
use status::{ENTRY_ATTEMPTS, MatcherStatus, STATUS_EVENT, Trouble};

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
/// After the library itself fails, the matcher waits this long.
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
    status: Mutex<MatcherStatus>,
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
            status: Mutex::new(MatcherStatus::Idle),
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

    /// Lets a paused or stopped matcher try again at once (Retry, or a setting it needs
    /// changed).
    pub fn resume(&self) {
        self.wake();
    }

    pub fn status(&self) -> MatcherStatus {
        lock(&self.0.status).clone()
    }

    fn set_status(&self, status: MatcherStatus) {
        {
            let mut current = lock(&self.0.status);
            if *current == status {
                return;
            }
            *current = status.clone();
        }
        if let Err(err) = self.0.app.emit(STATUS_EVENT, status) {
            log::warn!("cannot emit {STATUS_EVENT}: {err}");
        }
    }

    /// Asks YouTube without the browser's cookies: searches don't need them, and reading
    /// them can fail (Safari's, without Full Disk Access). Only when YouTube wants a
    /// signed-in user, again with them.
    async fn youtube<T, F, Fut>(&self, run: F) -> Result<T, AppError>
    where
        F: Fn(YtDlp) -> Fut,
        Fut: std::future::Future<Output = links::Result<T>>,
    {
        let signed_in = self.links()?.ytdlp()?;
        let plain = YtDlp {
            cookies_from_browser: None,
            ..signed_in.clone()
        };
        match run(plain).await {
            Err(LinkError::SignInRequired | LinkError::AgeRestricted)
                if signed_in.cookies_from_browser.is_some() =>
            {
                Ok(run(signed_in).await?)
            }
            result => Ok(result?),
        }
    }

    /// Makes the playlist, or brings `into` (else the one imported from `source_url`) up to
    /// date.
    async fn save(
        &self,
        name: String,
        source_url: Option<String>,
        into: Option<i64>,
        playlist: ImportedPlaylist,
    ) -> Result<ImportResult, AppError> {
        let library = self.library()?;
        let total = playlist.tracks.len();
        let partial = playlist.partial;
        let tracks = playlist.tracks;
        let (playlist, in_library, partial) = library
            .run(move |db| {
                let entries = match_in_library(db, tracks)?;
                let in_library = entries.iter().filter(|e| e.track_id.is_some()).count();
                let existing = match (into, &source_url) {
                    (Some(id), _) => Some(id),
                    (None, Some(url)) => db.imported_playlist(url)?,
                    (None, None) => None,
                };
                let playlist = match existing {
                    Some(id) => {
                        db.reimport_playlist(id, &entries, partial)?;
                        db.playlist(id)?
                            .ok_or_else(|| library::Error::Invalid("playlist vanished".into()))?
                    }
                    None => db.create_imported_playlist(&name, source_url.as_deref(), &entries)?,
                };
                // A partial page of a playlist that was completed before is no news.
                let partial = partial && db.import_progress(playlist.id)?.total <= total as i64;
                Ok((playlist, in_library, partial))
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

    /// Pasted text: links (one list, or tracks in bulk) or tracks written out. With `into`,
    /// the text completes that playlist (all of a list whose page showed only a part).
    pub async fn import_text(
        &self,
        text: &str,
        name: &str,
        into: Option<i64>,
    ) -> Result<ImportResult, AppError> {
        let items = import::text::parse(text);
        if items.is_empty() {
            return Err(AppError::Import("no tracks in the text".into()));
        }
        // One link alone: the list it points at.
        if let [Item::Link(url)] = items.as_slice()
            && into.is_none()
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
            into,
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
            results.push(self.save(name, None, None, playlist).await?);
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
        self.save(name, Some(link.url.clone()), None, playlist)
            .await
    }

    /// Playlists of the Music app (macOS), each kept as the playlist imported from it.
    pub async fn import_music(&self, ids: Vec<String>) -> Result<Vec<ImportResult>, AppError> {
        let mut results = Vec::new();
        for (id, playlist) in music::read(ids).await? {
            let name = playlist.name.clone().unwrap_or_default();
            results.push(
                self.save(name, Some(music::source_url(&id)), None, playlist)
                    .await?,
            );
        }
        Ok(results)
    }

    /// Reads a service's page for `link` (following Deezer's pages).
    async fn read_link(&self, link: &ServiceLink) -> Result<ImportedPlaylist, AppError> {
        let request = pages::request(link)
            .ok_or_else(|| AppError::Import("Vitra can't read lists from this link".into()))?;
        let response = http::get(&request.url, &request.headers, None).await?;
        if link.service == Service::YandexMusic {
            let read = if response.status == 404 {
                Err(PageError::NotFound)
            } else {
                pages::read(link, &response.body)
            };
            return match read {
                // «Мне нравится» and private playlists show only to their owner.
                Err(PageError::Private | PageError::NotFound) => {
                    self.read_signed_in(link, &request).await
                }
                other => other.map_err(page_error),
            };
        }
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
                let response = http::get(&url, &[], None).await?;
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

    /// Asks for a list again as the user, with the cookies of the browser chosen in Settings
    /// → Downloads (only Yandex Music's go along, and the file is gone right after).
    async fn read_signed_in(
        &self,
        link: &ServiceLink,
        request: &pages::Request,
    ) -> Result<ImportedPlaylist, AppError> {
        let links = self.links()?;
        let ytdlp = links.ytdlp()?;
        if ytdlp.cookies_from_browser.is_none() {
            return Err(AppError::Import(
                "the playlist is private on Yandex Music: choose the browser you're signed in \
                 with in Settings → Downloads → Cookies from browser, or make it public"
                    .into(),
            ));
        }
        std::fs::create_dir_all(links.scratch())?;
        let file = links
            .scratch()
            .join(format!("cookies-{}.txt", std::process::id()));
        let result = async {
            private_file(&file)?;
            ytdlp.export_cookies(&file).await?;
            keep_cookies_of(&file, "yandex.")?;
            let signed_in = pages::yandex_signed_in(request);
            let response = http::get(&signed_in.url, &signed_in.headers, Some(&file)).await?;
            pages::read(link, &response.body).map_err(page_error)
        }
        .await;
        let _ = std::fs::remove_file(&file);
        result
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
                self.store_link(url, &entry.source, Some(&candidate))
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
        let (mut found, library_paths): (Vec<EntryCandidate>, HashMap<i64, String>) = library
            .run(move |db| {
                let tracks = db.library_candidates(&lookup, LIBRARY_CANDIDATES)?;
                let candidates: Vec<Candidate> = tracks.iter().map(library_candidate).collect();
                let ranked: Vec<EntryCandidate> = matching::rank(&lookup, &candidates)
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
                    .collect();
                let paths = tracks.iter().map(|t| (t.id, t.path.clone())).collect();
                Ok((ranked, paths))
            })
            .await?;
        let online = self.search_online(&wanted, query.as_deref()).await?;
        found.extend(
            online
                .into_iter()
                .map(|(track, score)| entry_candidate(&track, score)),
        );
        // A link the library has shows once, as the library's.
        let known: Vec<String> = found
            .iter()
            .filter_map(|c| c.track_id)
            .filter_map(|id| library_paths.get(&id).cloned())
            .collect();
        found.retain(|c| c.track_id.is_some() || c.url.as_ref().is_none_or(|u| !known.contains(u)));
        Ok(found)
    }

    /// Looks again for the entries that weren't found.
    pub async fn retry(&self, playlist_id: i64) -> Result<(), AppError> {
        self.library()?
            .run(move |db| db.retry_entries(playlist_id, true))
            .await?;
        // The rows show the search again.
        self.announce(playlist_id, false).await?;
        self.wake();
        Ok(())
    }

    /// A link found for `source` as a library track (see [`library::Library::add_found_link`]),
    /// with the thumbnail its description brought as its cover.
    async fn store_link(
        &self,
        url: &str,
        source: &ImportedTrack,
        found: Option<&EntryCandidate>,
    ) -> Result<i64, AppError> {
        let library = self.library()?;
        let thumbnail = self.links().ok().and_then(|links| links.thumbnail(url));
        let covers = library.covers_dir().to_path_buf();
        let (url, source, found) = (url.to_owned(), source.clone(), found.cloned());
        library
            .run(move |db| {
                let (id, created) = db.add_found_link(&url, &source, found.as_ref())?;
                if created && let Some(image) = &thumbnail {
                    db.set_track_artwork(id, image, &covers)?;
                }
                Ok(id)
            })
            .await
    }

    /// Opens a link in full (names, length) and keeps its thumbnail for a while.
    async fn describe_link(&self, url: &str) -> Result<LinkTrack, AppError> {
        let links = self.links()?;
        self.pace().await;
        let scratch = links.scratch().to_path_buf();
        let (track, thumbnail) = self
            .youtube(|ytdlp| {
                let (url, scratch) = (url.to_owned(), scratch.clone());
                async move { ytdlp.describe(&url, &scratch).await }
            })
            .await?;
        if let Some(image) = thumbnail {
            links.remember_thumbnail(&track.url, image);
        }
        Ok(track)
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
        let query = query
            .map(str::to_owned)
            .unwrap_or_else(|| matching::search_query(wanted));
        self.pace().await;
        let results = self
            .youtube(|ytdlp| {
                let query = query.clone();
                async move { ytdlp.search(&query, SEARCH_RESULTS).await }
            })
            .await?;
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
            let track = match self.describe_link(&results[index].url).await {
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

    /// A YouTube Music song for a library track (the seed of My Wave's radio), if one
    /// matches at least doubtfully.
    pub async fn find_on_youtube(
        &self,
        wanted: &ImportedTrack,
    ) -> Result<Option<String>, AppError> {
        let found = self.search_online(wanted, None).await?;
        Ok(found
            .into_iter()
            .find(|(_, score)| Confidence::of(*score) != Confidence::None)
            .map(|(track, _)| track.url))
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
            let track = self.describe_link(&link.url).await?;
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
        // Outages in a row (each waits longer), and failed searches per entry.
        let mut outages = 0u32;
        let mut attempts: HashMap<i64, u32> = HashMap::new();
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
                attempts.clear();
                self.set_status(MatcherStatus::Idle);
                self.0.wake.notified().await;
                continue;
            }
            for entry in pending {
                // A bare link (Spotify's ⌘C) is named once its page is read.
                let title = match &entry.source.url {
                    Some(url) if entry.source.title.is_empty() => url.clone(),
                    _ => entry.source.display(),
                };
                self.set_status(MatcherStatus::Searching {
                    playlist_id: entry.playlist_id,
                    title,
                });
                let err = match self.find(&entry).await {
                    Ok(()) => {
                        outages = 0;
                        attempts.remove(&entry.id);
                        continue;
                    }
                    Err(err) => err,
                };
                log::info!("import: looking for {:?} failed: {err}", entry.source.title);
                match status::trouble(&err) {
                    // Nothing works until the settings change: say so and wait for that
                    // (or Retry, or another import).
                    Trouble::Blocked(reason) => {
                        self.set_status(MatcherStatus::Blocked {
                            reason,
                            detail: err.to_string(),
                        });
                        self.0.wake.notified().await;
                        break;
                    }
                    // Nothing answers for now: wait, longer each time, unless told to go on.
                    Trouble::Paused(reason) => {
                        outages += 1;
                        let wait = status::pause(outages);
                        self.set_status(MatcherStatus::Paused {
                            reason,
                            retry_at: unix_ms() + wait.as_millis() as u64,
                        });
                        tokio::select! {
                            () = tokio::time::sleep(wait) => {}
                            () = self.0.wake.notified() => {}
                        }
                        break;
                    }
                    // This entry's search failed: try the others, and after a few tries give
                    // it up as not found so it doesn't hold the queue.
                    Trouble::Entry => {
                        let tries = attempts.entry(entry.id).or_insert(0);
                        *tries += 1;
                        if *tries >= ENTRY_ATTEMPTS {
                            attempts.remove(&entry.id);
                            if let Err(err) = self.give_up(&entry).await {
                                log::warn!("import: {err}");
                            }
                        }
                    }
                }
            }
        }
    }

    /// Marks an entry that couldn't be looked for as not found (Look Again tries again).
    async fn give_up(&self, entry: &ImportEntry) -> Result<(), AppError> {
        let id = entry.id;
        self.library()?
            .run(move |db| db.resolve_entry(id, EntryState::Missing, None, &[]))
            .await?;
        self.announce(entry.playlist_id, false).await
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
                    return self.announce(entry.playlist_id, false).await;
                }
            }
        }
        // A link that is a track already plays as it is.
        if let Some(url) = &source.url
            && service_links::parse(url).is_some_and(|l| l.service == Service::Playable)
        {
            let id = entry.id;
            let track = self.store_link(url, &source, None).await?;
            library
                .run(move |db| db.resolve_entry(id, EntryState::Sure, Some(track), &[]))
                .await?;
            return self.announce(entry.playlist_id, true).await;
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
            return self.announce(entry.playlist_id, true).await;
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
        let track_id = match best.as_ref().map(|b| (b.track_id, b.url.as_deref())) {
            Some((Some(track_id), _)) => Some(track_id),
            Some((None, Some(url))) => Some(self.store_link(url, &source, best.as_ref()).await?),
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
        self.announce(entry.playlist_id, track_id.is_some()).await
    }

    /// Tells the windows an entry changed (and the library got a track, when `found`).
    async fn announce(&self, playlist_id: i64, found: bool) -> Result<(), AppError> {
        let library = self.library()?;
        if found {
            library.changed(Change::Tracks);
        }
        library.changed(Change::Playlists);
        self.emit_progress(playlist_id).await;
        Ok(())
    }
}

fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}

/// An empty file only the user can read (cookies go into it).
fn private_file(path: &Path) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map(drop)
}

/// Keeps only the cookies of `domain` in a Netscape cookie file.
fn keep_cookies_of(path: &Path, domain: &str) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    std::fs::write(path, cookies_of(&text, domain))
}

fn cookies_of(text: &str, domain: &str) -> String {
    text.lines()
        .filter(|line| {
            // `#HttpOnly_.site.com` lines are cookies; other `#` lines are comments.
            let cookie = line.strip_prefix("#HttpOnly_").unwrap_or(line);
            if cookie.starts_with('#') || cookie.trim().is_empty() {
                return true;
            }
            cookie
                .split('\t')
                .next()
                .is_some_and(|host| host.contains(domain))
        })
        .map(|line| format!("{line}\n"))
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_sites_cookies_go_along() {
        let jar = "# Netscape HTTP Cookie File\n\
                   .yandex.ru\tTRUE\t/\tTRUE\t2000000000\tSession_id\tabc\n\
                   #HttpOnly_.music.yandex.ru\tTRUE\t/\tTRUE\t0\tyandexuid\t1\n\
                   .google.com\tTRUE\t/\tTRUE\t2000000000\tSID\tsecret\n\
                   #HttpOnly_.bank.example\tTRUE\t/\tTRUE\t0\ttoken\tsecret\n";
        let kept = cookies_of(jar, "yandex.");
        assert!(kept.contains("Session_id"));
        assert!(kept.contains("yandexuid"));
        assert!(kept.starts_with("# Netscape"));
        assert!(!kept.contains("secret"));
    }

    #[test]
    fn typed_queries_go_either_way() {
        let query = split_query("Daft Punk - One More Time");
        assert_eq!(query.title, "One More Time");
        assert_eq!(query.artists, ["Daft Punk"]);
        assert_eq!(split_query("just words").title, "just words");
    }
}
