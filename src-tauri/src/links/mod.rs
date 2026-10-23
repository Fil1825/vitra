//! Links: resolving pasted URLs with yt-dlp, playing and importing them, downloading them
//! into the library and updating yt-dlp.
//!
//! Links join the library as tracks whose path is their web page (see `library::RemoteTrack`),
//! so favorites, playlists and history work for them; mpv plays them through its yt-dlp hook.

mod commands;
mod cookies;
mod downloads;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use library::{Playlist, RemoteTrack, Track};
use links::{LinkAlbum, LinkKind, LinkPlaylist, LinkTrack, Resolved, YtDlp};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::error::AppError;
use crate::library::{Change, LibraryService, entry_from_track};
use crate::player::{Entry, Player};
use crate::settings::SettingsStore;
use crate::sidecar::{self, Sidecar, SidecarSource};

pub use commands::*;
pub use downloads::{Downloads, GroupKind, GroupSpec};

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
    /// The artists when the site lists several.
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// The track's number on its album.
    pub track_number: Option<u32>,
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
        /// Set when the playlist is an album: the dialog shows it as one.
        album: Option<AlbumPreview>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumPreview {
    pub title: String,
    pub artist: Option<String>,
    pub year: Option<u16>,
    /// Tracks on the album, unavailable ones included.
    pub count: Option<u32>,
    /// The library album, once its tracks are in the library.
    pub album_id: Option<i64>,
}

/// Where an album's entries go when their full metadata comes in: the album, and each
/// entry's number by its id (or URL).
#[derive(Debug, Clone)]
struct Placement {
    album: LinkAlbum,
    numbers: HashMap<String, u32>,
}

impl Placement {
    fn of(playlist: &LinkPlaylist) -> Option<Self> {
        let album = playlist.album.clone()?;
        let numbers = playlist
            .tracks
            .iter()
            .filter_map(|track| Some((entry_key(track), track.track_number?)))
            .collect();
        Some(Self { album, numbers })
    }

    fn place(&self, track: &mut LinkTrack) {
        let number = self
            .numbers
            .get(&entry_key(track))
            .copied()
            .or(track.track_number);
        if let Some(number) = number {
            links::place_in_album(track, &self.album, number);
        }
    }
}

/// What identifies a playlist entry across its listing and its own page.
fn entry_key(track: &LinkTrack) -> String {
    track.id.clone().unwrap_or_else(|| track.url.clone())
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
    /// Albums looked for a cover this session (see [`LinkService::cover_album_later`]).
    covering: Mutex<HashSet<i64>>,
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
        artists: track.artists.clone(),
        album: track.album.clone(),
        album_artist: track.album_artist.clone(),
        track_number: track.track_number,
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
            covering: Mutex::new(HashSet::new()),
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
            cookies_from_browser: self
                .0
                .app
                .try_state::<SettingsStore>()
                .and_then(|settings| settings.get().downloads.cookies_browser()),
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
        let resolved = self.describe(&link).await?;
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

    /// What a link is now, with its thumbnail remembered.
    async fn describe(&self, link: &links::Link) -> Result<Resolved, AppError> {
        Ok(match link.kind {
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
        })
    }

    /// Takes a corrected link's details from its page again (`Library::restore_link`).
    pub async fn restore(&self, url: &str) -> Result<Track, AppError> {
        let link = links::parse_link(url)?;
        let Resolved::Track(mut track) = self.describe(&link).await? else {
            return Err(AppError::NotFound(format!("{url} is not a single track")));
        };
        let thumbnail = self.thumbnail(&track.url);
        // Kept under the URL the library has, whatever form yt-dlp reports now.
        url.clone_into(&mut track.url);
        let description = remote(&track, true);
        let library = self.library()?;
        let covers = library.covers_dir().to_path_buf();
        let restored = library
            .run(move |db| {
                let restored = db.restore_link(&description)?;
                let Some(image) = thumbnail else {
                    return Ok(restored);
                };
                db.set_track_artwork(restored.id, &image, &covers)?;
                Ok(db.track(restored.id)?.unwrap_or(restored))
            })
            .await?;
        self.tracks_updated(&library, vec![restored.clone()]);
        Ok(restored)
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
                let shown: Vec<LinkTrack> = playlist
                    .tracks
                    .iter()
                    .take(PREVIEW_TRACKS)
                    .cloned()
                    .collect();
                // An album's tracks may be in the library (or downloaded) already.
                let known: Vec<Option<Track>> = if playlist.album.is_some() {
                    let urls: Vec<String> = shown.iter().map(|t| t.url.clone()).collect();
                    library
                        .run(move |db| {
                            urls.iter()
                                .map(|url| db.track_by_source(url))
                                .collect::<library::Result<_>>()
                        })
                        .await?
                } else {
                    vec![None; shown.len()]
                };
                let mut tracks = Vec::new();
                for (track, known) in shown.iter().zip(&known) {
                    tracks.push(self.preview_track(track, known.as_ref()).await);
                }
                let album = playlist.album.as_ref().map(|album| AlbumPreview {
                    title: album.title.clone(),
                    artist: album.artist.clone(),
                    year: album.year,
                    count: album.count,
                    album_id: known.iter().flatten().find_map(|t| t.album_id),
                });
                let artwork = if album.is_some() {
                    self.album_artwork_id(&playlist).await
                } else {
                    self.artwork_id(&playlist.url).await
                };
                LinkPreview::Playlist {
                    artwork,
                    url: playlist.url,
                    title: album.as_ref().map_or(playlist.title, |a| a.title.clone()),
                    uploader: playlist.uploader,
                    count: playlist.tracks.len(),
                    tracks,
                    playlist_id,
                    album,
                }
            }
        })
    }

    /// A link the library has shows the library's details: the user may have corrected them.
    async fn preview_track(&self, track: &LinkTrack, known: Option<&Track>) -> PreviewTrack {
        let mut preview = PreviewTrack {
            url: track.url.clone(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            track_number: track.track_number,
            duration: track.duration,
            live: track.live,
            artwork: self.artwork_id(&track.url).await,
            track_id: known.map(|t| t.id),
            downloaded: known.is_some_and(|t| !links::is_remote(&t.path)),
        };
        if let Some(known) = known {
            preview.title.clone_from(&known.title);
            preview.artist.clone_from(&known.artist);
            preview.artists = known.artists.iter().map(|a| a.name.clone()).collect();
            preview.album.clone_from(&known.album);
        }
        preview
    }

    /// An album's cover: the playlist's thumbnail, else its first track's, cut square (see
    /// [`media::square_cover`]). Remembered, so every track of a download gets the same one.
    pub async fn album_cover(&self, playlist: &LinkPlaylist) -> Option<Arc<Vec<u8>>> {
        let key = format!("{}#album", playlist.url);
        if let Some(cover) = self.thumbnail(&key) {
            return Some(cover);
        }
        let source = self.thumbnail(&playlist.url).or_else(|| {
            playlist
                .tracks
                .iter()
                .find_map(|track| self.thumbnail(&track.url))
        })?;
        let square = tauri::async_runtime::spawn_blocking(move || media::square_cover(&source))
            .await
            .ok()?
            .inspect_err(|err| log::info!("cannot read an album cover: {err}"))
            .ok()?;
        let square = Arc::new(square);
        remember(&self.0.thumbnails, &key, Arc::clone(&square));
        Some(square)
    }

    async fn album_artwork_id(&self, playlist: &LinkPlaylist) -> Option<String> {
        let bytes = self.album_cover(playlist).await?;
        self.prepared_artwork(bytes).await
    }

    /// Puts a link's thumbnail into the player's artwork store for the dialog to show.
    async fn artwork_id(&self, url: &str) -> Option<String> {
        let bytes = self.thumbnail(url)?;
        self.prepared_artwork(bytes).await
    }

    async fn prepared_artwork(&self, bytes: Arc<Vec<u8>>) -> Option<String> {
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

    /// Adds an album's tracks to the library as one album with its cover, and fills in their
    /// metadata in the background.
    async fn store_album(&self, playlist: &LinkPlaylist) -> Result<Vec<Track>, AppError> {
        let library = self.library()?;
        let tracks: Vec<RemoteTrack> = playlist.tracks.iter().map(|t| remote(t, false)).collect();
        let cover = self.album_cover(playlist).await;
        let covers = library.covers_dir().to_path_buf();
        let stored = library
            .run(move |db| {
                let stored = db.add_remote_tracks(&tracks)?;
                let albums: HashSet<i64> = stored.iter().filter_map(|t| t.album_id).collect();
                if let Some(cover) = cover {
                    for album in &albums {
                        db.set_album_artwork(*album, &cover, &covers)?;
                    }
                }
                let ids = stored.iter().map(|t| t.id).collect();
                db.tracks(&library::TrackFilter::Ids(ids))
            })
            .await?;
        library.changed(Change::Tracks);
        self.enrich(playlist.url.clone(), Placement::of(playlist));
        Ok(stored)
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
            Resolved::Track(track) => {
                // A link the library has already plays as that track, corrections included.
                let known = match self.library() {
                    Ok(library) => {
                        let url = track.url.clone();
                        library
                            .run(move |db| db.track_by_source(&url))
                            .await
                            .ok()
                            .flatten()
                    }
                    Err(_) => None,
                };
                vec![known.map_or_else(|| link_entry(&track), entry_from_track)]
            }
            Resolved::Playlist(playlist) if playlist.album.is_some() => self
                .store_album(&playlist)
                .await?
                .into_iter()
                .map(entry_from_track)
                .collect(),
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
        self.enrich(playlist.url, None);
        Ok(imported)
    }

    /// Fills in the entries of an imported playlist (a listing has little more than titles)
    /// with their full metadata and thumbnails, in the background.
    fn enrich(&self, url: String, placement: Option<Placement>) {
        if !lock(&self.0.enriching).insert(url.clone()) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = service.run_enrich(&url, placement.as_ref()).await {
                log::warn!("cannot fill in the playlist {url}: {err}");
            }
            lock(&service.0.enriching).remove(&url);
        });
    }

    /// Album entries are filed under their album and numbered; their thumbnails aren't their
    /// own covers (the album's shows for them), but the first one covers an album that has
    /// none yet.
    async fn run_enrich(&self, url: &str, placement: Option<&Placement>) -> Result<(), AppError> {
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
        while let Some((mut track, thumbnail)) = receiver.recv().await {
            if let Some(thumbnail) = thumbnail {
                remember(&self.0.thumbnails, &track.url, Arc::new(thumbnail));
            }
            if let Some(placement) = placement {
                placement.place(&mut track);
            }
            let stored = self
                .store_quietly(&library, remote(&track, true), placement.is_some())
                .await?;
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

    /// Like [`Self::store_tracks`] without telling the views (enrichment batches that). An
    /// album entry's thumbnail covers its album if that has no cover yet (see
    /// [`library::Library::store_enriched`]).
    async fn store_quietly(
        &self,
        library: &LibraryService,
        track: RemoteTrack,
        album_entry: bool,
    ) -> Result<Option<Track>, AppError> {
        let thumbnail = self.thumbnail(&track.url);
        let covers = library.covers_dir().to_path_buf();
        library
            .run(move |db| {
                db.store_enriched(
                    &track,
                    thumbnail.as_deref().map(Vec::as_slice),
                    album_entry,
                    &covers,
                )
            })
            .await
    }

    /// Looks for a cover for a link album that has none, in the background: one stored
    /// before entries' thumbnails covered albums, or whose listing and entries came without
    /// one. A thumbnail seen recently is used, else the first entry's is asked of yt-dlp.
    /// Tried once per album and session; the views reload if it worked.
    pub fn cover_album_later(&self, album_id: i64) {
        if !lock(&self.0.covering).insert(album_id) {
            return;
        }
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = service.cover_album(album_id).await {
                log::info!("cannot find a cover for album {album_id}: {err}");
            }
        });
    }

    async fn cover_album(&self, album_id: i64) -> Result<(), AppError> {
        let library = self.library()?;
        let links = library.run(move |db| db.bare_link_album(album_id)).await?;
        let Some(first) = links.first() else {
            return Ok(());
        };
        let thumbnail = match links.iter().find_map(|url| self.thumbnail(url)) {
            Some(thumbnail) => thumbnail,
            None => {
                let (_, thumbnail) = self.ytdlp()?.resolve(first, &self.0.scratch).await?;
                let Some(thumbnail) = thumbnail else {
                    return Ok(());
                };
                let thumbnail = Arc::new(thumbnail);
                remember(&self.0.thumbnails, first, Arc::clone(&thumbnail));
                thumbnail
            }
        };
        let covers = library.covers_dir().to_path_buf();
        let cover = library
            .run(move |db| db.cover_bare_album(album_id, &thumbnail, &covers))
            .await?;
        if cover.is_some() {
            library.changed(Change::Tracks);
            library.changed(Change::Playlists);
        }
        Ok(())
    }

    fn tracks_updated(&self, library: &LibraryService, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        if let Some(player) = self.player() {
            player.library_tracks_updated(&tracks);
        }
    }

    /// Library tracks for links (added if needed), for downloading, and the album or playlist
    /// they are downloaded as.
    pub async fn tracks_for_download(
        &self,
        url: &str,
    ) -> Result<(Vec<Track>, Option<GroupSpec>), AppError> {
        match self.resolved(url).await? {
            Resolved::Track(track) => {
                Ok((self.store_tracks(vec![remote(&track, true)]).await?, None))
            }
            Resolved::Playlist(playlist) => {
                let (tracks, kind) = match &playlist.album {
                    Some(_) => (self.store_album(&playlist).await?, GroupKind::Album),
                    None => (
                        self.import_playlist(playlist.clone()).await?.1,
                        GroupKind::Playlist,
                    ),
                };
                let cover = match kind {
                    GroupKind::Album => self.album_cover(&playlist).await,
                    GroupKind::Playlist => None,
                };
                let album = playlist.album.clone();
                let group = GroupSpec {
                    url: playlist.url.clone(),
                    kind,
                    title: album
                        .as_ref()
                        .map_or_else(|| playlist.title.clone(), |a| a.title.clone()),
                    artist: album
                        .as_ref()
                        .and_then(|a| a.artist.clone())
                        .or(playlist.uploader.clone()),
                    cover_id: tracks.iter().find_map(|t| t.cover.clone()),
                    album,
                    cover,
                    first_track: playlist.tracks.first().map(|t| t.url.clone()),
                };
                Ok((tracks, Some(group)))
            }
        }
    }

    /// The streamed tracks of a library album, to download as that album: into the album
    /// template's folders, with album tags and one cover. The cover is found the way a
    /// link album's is when it has none (the first track's thumbnail, else the art yt-dlp
    /// embeds): a cover the user chose stays in the library.
    pub async fn album_for_download(
        &self,
        album_id: i64,
    ) -> Result<(Vec<Track>, Option<GroupSpec>), AppError> {
        let library = self.library()?;
        let (album, tracks) = library
            .run(move |db| {
                Ok((
                    db.album(album_id)?,
                    db.tracks(&library::TrackFilter::Album(album_id))?,
                ))
            })
            .await?;
        let album = album.ok_or_else(|| AppError::NotFound(format!("no album {album_id}")))?;
        // "3 of 12": the album's last number, or how many tracks it has.
        let count = tracks
            .iter()
            .filter_map(|t| t.track_number)
            .max()
            .unwrap_or(0)
            .max(i64::try_from(tracks.len()).unwrap_or(i64::MAX));
        let streamed: Vec<Track> = tracks
            .into_iter()
            .filter(|t| t.source_url.as_deref() == Some(t.path.as_str()))
            .collect();
        let Some(first) = streamed.first() else {
            return Ok((Vec::new(), None));
        };
        let group = GroupSpec {
            // Only identifies the group: an album of the library has no link of its own.
            url: format!("vitra:album/{album_id}"),
            kind: GroupKind::Album,
            title: album.title.clone(),
            artist: album.artist.clone(),
            cover_id: album.cover.clone(),
            album: Some(LinkAlbum {
                title: album.title,
                artist: album.artist,
                year: album.year.and_then(|year| u16::try_from(year).ok()),
                count: u32::try_from(count).ok(),
            }),
            cover: None,
            first_track: Some(first.path.clone()),
        };
        Ok((streamed, Some(group)))
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
            // On macOS Vitra installs the release itself; elsewhere a copy updates itself.
            updatable: cfg!(target_os = "macos")
                || found.is_some_and(|found| found.source != SidecarSource::System),
            ffmpeg: paths.locate(Sidecar::Ffmpeg).is_some(),
        }
    }

    /// Runs yt-dlp once in the background, so the first link doesn't wait for macOS to check
    /// a freshly installed yt-dlp (a one-time ~25 s on first launch).
    pub fn warm_up(&self) {
        let Ok(ytdlp) = self.ytdlp() else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            let started = Instant::now();
            match ytdlp.version().await {
                Ok(version) => log::info!(
                    "yt-dlp {version} at {} answered in {:.1} s",
                    ytdlp.binary.display(),
                    started.elapsed().as_secs_f64()
                ),
                Err(err) => log::warn!("yt-dlp at {} failed: {err}", ytdlp.binary.display()),
            }
        });
    }

    /// Updates yt-dlp and returns the new version. On macOS the latest release's folder build
    /// is downloaded into the app data folder (it can't update itself); elsewhere the bundled
    /// binary is copied there first (an app bundle may be read-only or signed), then
    /// `yt-dlp -U` replaces that copy.
    pub async fn update_ytdlp(&self) -> Result<String, AppError> {
        let _updating = self.0.updating.lock().await;
        if cfg!(target_os = "macos") {
            let sidecars = self.0.app.path().app_data_dir()?.join("sidecars");
            let folder = sidecars.join(sidecar::YT_DLP_MACOS);
            let Some(dir) = folder.parent() else {
                return Err(AppError::NotFound(sidecar::YT_DLP_MACOS.into()));
            };
            let installed = links::release::install_zip_release(
                links::release::LATEST_RELEASE,
                "yt-dlp_macos.zip",
                dir,
            )
            .await?;
            log::info!(
                "yt-dlp in {}: {}",
                dir.display(),
                if installed {
                    "installed the latest release"
                } else {
                    "already the latest"
                }
            );
            let ytdlp = YtDlp {
                binary: folder,
                ..self.ytdlp()?
            };
            // The first run also gets the system's one-time check over with.
            return Ok(ytdlp.version().await?);
        }
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
    fn album_entries_are_placed_by_their_listing_numbers() {
        let listed = |id: &str, n: u32| LinkTrack {
            url: format!("https://y/{id}"),
            id: Some(id.into()),
            title: id.into(),
            track_number: Some(n),
            ..LinkTrack::default()
        };
        let playlist = LinkPlaylist {
            url: "https://y/list".into(),
            id: None,
            title: "Album - X".into(),
            uploader: None,
            tracks: vec![listed("a", 1), listed("c", 3)],
            album: Some(LinkAlbum {
                title: "X".into(),
                artist: Some("Band".into()),
                year: None,
                count: Some(3),
            }),
        };
        let placement = Placement::of(&playlist).unwrap();
        // The page of the third track, with the page's own (different) album name.
        let mut page = LinkTrack {
            url: "https://y/c".into(),
            id: Some("c".into()),
            album: Some("X (Deluxe)".into()),
            year: Some(2020),
            ..LinkTrack::default()
        };
        placement.place(&mut page);
        assert_eq!(page.track_number, Some(3));
        assert_eq!(page.album.as_deref(), Some("X"));
        assert_eq!(page.album_artist.as_deref(), Some("Band"));
        assert_eq!(page.year, Some(2020));
        // An ordinary playlist has no placement.
        assert!(
            Placement::of(&LinkPlaylist {
                album: None,
                ..playlist
            })
            .is_none()
        );
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
        assert!(stored.artists.is_empty());
        assert!(!stored.complete);
        let entry = link_entry(&track);
        assert_eq!(entry.path, "https://y/1");
        assert_eq!(entry.track_id, None);
        assert_eq!(entry.duration, Some(10.0));
    }
}
