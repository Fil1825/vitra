//! Links: resolving pasted URLs with yt-dlp, playing and importing them, downloading them
//! into the library and updating yt-dlp.
//!
//! Links join the library as tracks whose path is their web page (see `library::RemoteTrack`),
//! so favorites, playlists and history work for them; mpv plays them through its yt-dlp hook.

mod commands;
mod downloads;

use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use library::{Playlist, RemoteTrack, Track};
use links::{LinkKind, LinkPlaylist, LinkTrack, Resolved, YtDlp};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::library::{Change, LibraryService, entry_from_track};
use crate::player::{Entry, Player};
use crate::settings::SettingsStore;
use crate::sidecar::{self, Sidecar, SidecarSource};

pub use commands::*;
pub use downloads::Downloads;

/// Resolutions and thumbnails kept for links seen recently.
const RECENT: usize = 32;
/// Tracks of a playlist sent with its preview; the rest are imported all the same.
const PREVIEW_TRACKS: usize = 100;
/// While a playlist's entries are filled in, the views refresh this often.
const ENRICH_REFRESH: Duration = Duration::from_secs(2);

/// How links join the queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayMode {
    /// Replace the queue.
    Play,
    Next,
    Append,
}

/// A track as the link dialog shows it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTrack {
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub live: bool,
    /// Served at `artwork://localhost/<id>`.
    pub artwork: Option<String>,
    /// The library track, once the link is in the library.
    pub track_id: Option<i64>,
    /// The library has it as a downloaded file.
    pub downloaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LinkPreview {
    Track {
        track: PreviewTrack,
    },
    #[serde(rename_all = "camelCase")]
    Playlist {
        url: String,
        title: String,
        uploader: Option<String>,
        artwork: Option<String>,
        count: usize,
        /// The first entries.
        tracks: Vec<PreviewTrack>,
        /// The playlist this link was imported as before.
        playlist_id: Option<i64>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YtDlpInfo {
    pub version: Option<String>,
    pub path: Option<String>,
    /// Whether "Update" can work: the copy in the app data folder updates itself.
    pub updatable: bool,
    pub ffmpeg: bool,
}

#[derive(Clone)]
pub struct LinkService(Arc<Inner>);

struct Inner {
    app: AppHandle,
    /// Temporary files of yt-dlp runs (thumbnails).
    scratch: PathBuf,
    /// Recent resolutions by URL, so acting on a preview doesn't ask yt-dlp again.
    resolved: Mutex<VecDeque<(String, Resolved)>>,
    /// Recent thumbnails by link URL: the player shows them as artwork.
    thumbnails: Mutex<VecDeque<(String, Arc<Vec<u8>>)>>,
    /// Playlists whose entries are being filled in.
    enriching: Mutex<HashSet<String>>,
    updating: tokio::sync::Mutex<()>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Keeps the newest `RECENT` values per key.
fn remember<T>(list: &Mutex<VecDeque<(String, T)>>, key: &str, value: T) {
    let mut list = lock(list);
    list.retain(|(k, _)| k != key);
    list.push_back((key.to_owned(), value));
    while list.len() > RECENT {
        list.pop_front();
    }
}

fn recall<T: Clone>(list: &Mutex<VecDeque<(String, T)>>, key: &str) -> Option<T> {
    lock(list)
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

/// A link track for the library. Playlist listings only describe entries in part.
fn remote(track: &LinkTrack, complete: bool) -> RemoteTrack {
    RemoteTrack {
        url: track.url.clone(),
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        year: track.year,
        duration: track.duration,
        complete,
    }
}

/// A queue entry for a link that stays out of the library.
fn link_entry(track: &LinkTrack) -> Entry {
    Entry {
        path: track.url.clone(),
        track_id: None,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: track.album.clone(),
        duration: track.duration,
        cover: None,
    }
}

impl LinkService {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        let cache = app.path().app_cache_dir()?;
        let scratch = cache.join("links");
        // Leftovers of a run that crashed.
        let _ = std::fs::remove_dir_all(&scratch);
        Ok(Self(Arc::new(Inner {
            app,
            scratch,
            resolved: Mutex::new(VecDeque::new()),
            thumbnails: Mutex::new(VecDeque::new()),
            enriching: Mutex::new(HashSet::new()),
            updating: tokio::sync::Mutex::new(()),
        })))
    }

    fn library(&self) -> Result<LibraryService, AppError> {
        self.0
            .app
            .try_state::<LibraryService>()
            .map(|library| library.inner().clone())
            .ok_or_else(|| AppError::Player("the library is not available".into()))
    }

    fn player(&self) -> Option<Player> {
        self.0
            .app
            .try_state::<Player>()
            .map(|player| player.inner().clone())
    }

    fn adds_links_to_library(&self) -> bool {
        self.0
            .app
            .try_state::<SettingsStore>()
            .is_none_or(|settings| settings.get().library.add_opened_files)
    }

    /// yt-dlp with FFmpeg and Deno, where they are installed.
    pub fn ytdlp(&self) -> Result<YtDlp, AppError> {
        let paths = sidecar::search_paths(&self.0.app);
        let binary = paths
            .locate(Sidecar::YtDlp)
            .ok_or(links::LinkError::YtDlpMissing)?
            .path;
        Ok(YtDlp {
            binary,
            // yt-dlp also needs ffprobe; given the directory, it finds both there.
            ffmpeg: paths
                .locate(Sidecar::Ffmpeg)
                .and_then(|found| found.path.parent().map(Path::to_path_buf)),
            deno: paths.locate(Sidecar::Deno).map(|found| found.path),
        })
    }

    /// The thumbnail of a link seen recently.
    pub fn thumbnail(&self, url: &str) -> Option<Arc<Vec<u8>>> {
        recall(&self.0.thumbnails, url)
    }

    /// What a pasted link is. Pages are asked about with yt-dlp; direct files and radio
    /// streams are taken as they are (mpv reads their tags while playing).
    pub async fn resolve(&self, input: &str) -> Result<LinkPreview, AppError> {
        let link = links::parse_link(input)?;
        let resolved = match link.kind {
            LinkKind::Audio | LinkKind::Stream => Resolved::Track(LinkTrack {
                title: links::link_title(&link.url),
                live: link.kind == LinkKind::Stream,
                url: link.url.clone(),
                ..LinkTrack::default()
            }),
            LinkKind::Page => {
                let (resolved, thumbnail) =
                    self.ytdlp()?.resolve(&link.url, &self.0.scratch).await?;
                if let Some(thumbnail) = thumbnail {
                    let url = match &resolved {
                        Resolved::Track(track) => track.url.clone(),
                        Resolved::Playlist(playlist) => playlist.url.clone(),
                    };
                    remember(&self.0.thumbnails, &url, Arc::new(thumbnail));
                }
                resolved
            }
        };
        let key = match &resolved {
            Resolved::Track(track) => track.url.clone(),
            Resolved::Playlist(playlist) => playlist.url.clone(),
        };
        remember(&self.0.resolved, &key, resolved.clone());
        // The pasted form may differ from the canonical one yt-dlp reports.
        if key != link.url {
            remember(&self.0.resolved, &link.url, resolved.clone());
        }
        self.preview(resolved).await
    }

    async fn preview(&self, resolved: Resolved) -> Result<LinkPreview, AppError> {
        let library = self.library()?;
        Ok(match resolved {
            Resolved::Track(track) => {
                let url = track.url.clone();
                let known = library.run(move |db| db.track_by_source(&url)).await?;
                LinkPreview::Track {
                    track: self.preview_track(&track, known.as_ref()).await,
                }
            }
            Resolved::Playlist(playlist) => {
                let url = playlist.url.clone();
                let playlist_id = library
                    .run(move |db| db.playlist_by_source(&url))
                    .await?
                    .map(|p| p.id);
                let mut tracks = Vec::new();
                for track in playlist.tracks.iter().take(PREVIEW_TRACKS) {
                    tracks.push(self.preview_track(track, None).await);
                }
                LinkPreview::Playlist {
                    artwork: self.artwork_id(&playlist.url).await,
                    url: playlist.url,
                    title: playlist.title,
                    uploader: playlist.uploader,
                    count: playlist.tracks.len(),
                    tracks,
                    playlist_id,
                }
            }
        })
    }

    async fn preview_track(&self, track: &LinkTrack, known: Option<&Track>) -> PreviewTrack {
        PreviewTrack {
            url: track.url.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration: track.duration,
            live: track.live,
            artwork: self.artwork_id(&track.url).await,
            track_id: known.map(|t| t.id),
            downloaded: known.is_some_and(|t| !links::is_remote(&t.path)),
        }
    }

    /// Puts a link's thumbnail into the player's artwork store for the dialog to show.
    async fn artwork_id(&self, url: &str) -> Option<String> {
        let bytes = self.thumbnail(url)?;
        let player = self.player()?;
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            media::prepare_artwork(media::Artwork {
                bytes: bytes.as_ref().clone(),
                mime: None,
            })
        })
        .await
        .ok()?
        .inspect_err(|err| log::info!("cannot read a thumbnail: {err}"))
        .ok()?;
        Some(player.artwork().insert(prepared))
    }

    /// A link resolved recently, or resolved now.
    async fn resolved(&self, url: &str) -> Result<Resolved, AppError> {
        if let Some(resolved) = recall(&self.0.resolved, url) {
            return Ok(resolved);
        }
        self.resolve(url).await?;
        recall(&self.0.resolved, url)
            .ok_or_else(|| AppError::NotFound(format!("cannot resolve {url}")))
    }

    /// Adds link tracks to the library with their thumbnails as covers.
    async fn store_tracks(&self, tracks: Vec<RemoteTrack>) -> Result<Vec<Track>, AppError> {
        let library = self.library()?;
        let thumbnails: Vec<Option<Arc<Vec<u8>>>> =
            tracks.iter().map(|t| self.thumbnail(&t.url)).collect();
        let covers = library.covers_dir().to_path_buf();
        let stored = library
            .run(move |db| {
                let stored = db.add_remote_tracks(&tracks)?;
                let mut covered = Vec::with_capacity(stored.len());
                for (track, thumbnail) in stored.into_iter().zip(thumbnails) {
                    // Downloaded tracks have their cover in the file.
                    if let Some(image) = thumbnail.filter(|_| links::is_remote(&track.path)) {
                        db.set_track_artwork(track.id, &image, &covers)?;
                        covered.push(db.track(track.id)?.unwrap_or(track));
                    } else {
                        covered.push(track);
                    }
                }
                Ok(covered)
            })
            .await?;
        library.changed(Change::Tracks);
        Ok(stored)
    }

    /// Plays or queues a link (a track, or every entry of a playlist, which is imported).
    pub async fn play(&self, url: &str, mode: PlayMode) -> Result<(), AppError> {
        let entries = match self.resolved(url).await? {
            Resolved::Track(track) if self.adds_links_to_library() => self
                .store_tracks(vec![remote(&track, true)])
                .await?
                .into_iter()
                .map(entry_from_track)
                .collect(),
            Resolved::Track(track) => vec![link_entry(&track)],
            Resolved::Playlist(playlist) => {
                let (_, tracks) = self.import_playlist(playlist).await?;
                tracks.into_iter().map(entry_from_track).collect()
            }
        };
        let player = self
            .player()
            .ok_or_else(|| AppError::Player("the player is not running".into()))?;
        match mode {
            PlayMode::Play => player.play_entries(entries, 0).await,
            PlayMode::Next => player.enqueue(entries, true).await,
            PlayMode::Append => player.enqueue(entries, false).await,
        }
    }

    /// Imports a playlist link as a library playlist.
    pub async fn import(&self, url: &str) -> Result<Playlist, AppError> {
        match self.resolved(url).await? {
            Resolved::Playlist(playlist) => Ok(self.import_playlist(playlist).await?.0),
            Resolved::Track(_) => Err(AppError::NotFound("not a playlist".into())),
        }
    }

    async fn import_playlist(
        &self,
        playlist: LinkPlaylist,
    ) -> Result<(Playlist, Vec<Track>), AppError> {
        let library = self.library()?;
        let tracks: Vec<RemoteTrack> = playlist.tracks.iter().map(|t| remote(t, false)).collect();
        let (name, url) = (playlist.title.clone(), playlist.url.clone());
        let imported = library
            .run(move |db| db.import_playlist(&name, &url, &tracks))
            .await?;
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        self.enrich(playlist.url);
        Ok(imported)
    }

    /// Fills in the entries of an imported playlist (a listing has little more than titles)
    /// with their full metadata and thumbnails, in the background.
    fn enrich(&self, url: String) {
        if !lock(&self.0.enriching).insert(url.clone()) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = service.run_enrich(&url).await {
                log::warn!("cannot fill in the playlist {url}: {err}");
            }
            lock(&service.0.enriching).remove(&url);
        });
    }

    async fn run_enrich(&self, url: &str) -> Result<(), AppError> {
        let ytdlp = self.ytdlp()?;
        let library = self.library()?;
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let scratch = self.0.scratch.clone();
        let url = url.to_owned();
        let run = tauri::async_runtime::spawn(async move {
            ytdlp
                .enrich(&url, &scratch, |track, thumbnail| {
                    let _ = sender.send((track, thumbnail));
                })
                .await
        });
        let mut last_refresh = Instant::now();
        let mut pending: Vec<Track> = Vec::new();
        while let Some((track, thumbnail)) = receiver.recv().await {
            if let Some(thumbnail) = thumbnail {
                remember(&self.0.thumbnails, &track.url, Arc::new(thumbnail));
            }
            let stored = self.store_quietly(&library, remote(&track, true)).await?;
            pending.extend(stored);
            if last_refresh.elapsed() >= ENRICH_REFRESH {
                last_refresh = Instant::now();
                self.tracks_updated(&library, std::mem::take(&mut pending));
            }
        }
        self.tracks_updated(&library, pending);
        run.await??;
        Ok(())
    }

    /// Like [`Self::store_tracks`] without telling the views (enrichment batches that).
    async fn store_quietly(
        &self,
        library: &LibraryService,
        track: RemoteTrack,
    ) -> Result<Option<Track>, AppError> {
        let thumbnail = self.thumbnail(&track.url);
        let covers = library.covers_dir().to_path_buf();
        library
            .run(move |db| {
                let Some(stored) = db.add_remote_tracks(&[track])?.pop() else {
                    return Ok(None);
                };
                if let Some(image) = thumbnail.filter(|_| links::is_remote(&stored.path)) {
                    db.set_track_artwork(stored.id, &image, &covers)?;
                }
                db.track(stored.id)
            })
            .await
    }

    fn tracks_updated(&self, library: &LibraryService, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        if let Some(player) = self.player() {
            player.remote_tracks_updated(&tracks);
        }
    }

    /// Library tracks for links (added if needed), for downloading.
    pub async fn tracks_for_download(&self, url: &str) -> Result<Vec<Track>, AppError> {
        match self.resolved(url).await? {
            Resolved::Track(track) => self.store_tracks(vec![remote(&track, true)]).await,
            Resolved::Playlist(playlist) => Ok(self.import_playlist(playlist).await?.1),
        }
    }

    pub async fn ytdlp_info(&self) -> YtDlpInfo {
        let paths = sidecar::search_paths(&self.0.app);
        let found = paths.locate(Sidecar::YtDlp);
        let version = match self.ytdlp() {
            Ok(ytdlp) => ytdlp.version().await.ok(),
            Err(_) => None,
        };
        YtDlpInfo {
            version,
            path: found
                .as_ref()
                .map(|found| found.path.to_string_lossy().into_owned()),
            updatable: found.is_some_and(|found| found.source != SidecarSource::System),
            ffmpeg: paths.locate(Sidecar::Ffmpeg).is_some(),
        }
    }

    /// Updates yt-dlp: the bundled binary is copied into the app data folder first (an app
    /// bundle may be read-only or signed), then `yt-dlp -U` replaces that copy. Returns the
    /// new version.
    pub async fn update_ytdlp(&self) -> Result<String, AppError> {
        let _updating = self.0.updating.lock().await;
        let paths = sidecar::search_paths(&self.0.app);
        let found = paths
            .locate(Sidecar::YtDlp)
            .ok_or(links::LinkError::YtDlpMissing)?;
        let binary = match found.source {
            SidecarSource::AppData | SidecarSource::System => found.path,
            SidecarSource::Bundled | SidecarSource::Dev => {
                let dir = self.0.app.path().app_data_dir()?.join("sidecars");
                let copy = dir.join(Sidecar::YtDlp.file_name());
                let source = found.path.clone();
                let target = copy.clone();
                tauri::async_runtime::spawn_blocking(move || copy_executable(&source, &target))
                    .await??;
                log::info!("copied yt-dlp to {}", copy.display());
                copy
            }
        };
        let ytdlp = YtDlp {
            binary,
            ..self.ytdlp()?
        };
        let report = ytdlp.update().await?;
        log::info!("yt-dlp update: {report}");
        Ok(ytdlp.version().await?)
    }
}

/// Copies a program, through a temporary file so a half-written copy is never run.
fn copy_executable(source: &Path, target: &Path) -> std::io::Result<()> {
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let partial = target.with_extension("part");
    std::fs::copy(source, &partial)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&partial, target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_values_are_capped_and_refreshed() {
        let list = Mutex::new(VecDeque::new());
        for n in 0..RECENT + 5 {
            remember(&list, &n.to_string(), n);
        }
        assert_eq!(recall(&list, "0"), None);
        assert_eq!(recall(&list, &(RECENT + 4).to_string()), Some(RECENT + 4));
        remember(&list, "7", 70);
        assert_eq!(recall(&list, "7"), Some(70));
        assert_eq!(lock(&list).len(), RECENT);
    }

    #[test]
    fn copies_programs_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("yt-dlp");
        std::fs::write(&source, b"#!/bin/sh\n").unwrap();
        let target = tmp.path().join("app/sidecars/yt-dlp");
        copy_executable(&source, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"#!/bin/sh\n");
        assert!(!target.with_extension("part").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&target).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111);
        }
    }

    #[test]
    fn link_tracks_become_library_tracks_and_entries() {
        let track = LinkTrack {
            url: "https://y/1".into(),
            title: "Song".into(),
            artist: Some("Artist".into()),
            duration: Some(10.0),
            ..LinkTrack::default()
        };
        let stored = remote(&track, false);
        assert_eq!(stored.url, "https://y/1");
        assert!(!stored.complete);
        let entry = link_entry(&track);
        assert_eq!(entry.path, "https://y/1");
        assert_eq!(entry.track_id, None);
        assert_eq!(entry.duration, Some(10.0));
    }
}
