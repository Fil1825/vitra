//! The download queue: links downloaded into the library a few at a time, with progress,
//! cancelling and retrying. A finished download takes over its streamed track's row.
//!
//! The tracks of an album or a playlist downloaded together form a group, which the windows
//! show as one item with the overall progress and cancel or retry as a whole. Album tracks go
//! into the album template's folders and get the album's tags and one cover.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use library::Track;
use links::{DownloadEvent, DownloadOptions, DownloadStatus, FileNameFields, LinkAlbum, LinkError};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::watch;

use super::LinkService;
use crate::error::AppError;
use crate::library::{Change, LibraryService};
use crate::player::Player;
use crate::settings::{DownloadSettings, SettingsStore};

pub const DOWNLOADS_EVENT: &str = "downloads://changed";

/// Downloads running at once: enough to keep the line busy, few enough not to get
/// throttled by the site.
const PARALLEL: usize = 2;
/// Progress is sent to the windows at most this often.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupKind {
    Album,
    Playlist,
}

/// An album or playlist downloaded together, as the windows see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupInfo {
    pub id: u64,
    pub kind: GroupKind,
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    /// Library cover id (`artwork://localhost/cover/<id>`).
    pub cover: Option<String>,
}

/// What a group is made from.
#[derive(Debug, Clone)]
pub struct GroupSpec {
    pub url: String,
    pub kind: GroupKind,
    pub title: String,
    pub artist: Option<String>,
    pub cover_id: Option<String>,
    /// The album, for album downloads.
    pub album: Option<LinkAlbum>,
    /// The album's square cover, written into every track.
    pub cover: Option<Arc<Vec<u8>>>,
    /// The album's first track: its thumbnail is the cover when the album had none.
    pub first_track: Option<String>,
}

struct Group {
    info: GroupInfo,
    album: Option<LinkAlbum>,
    /// Found lazily when the album came without one; then every later track gets it too.
    cover: Mutex<Option<Arc<Vec<u8>>>>,
    first_track: Option<String>,
}

/// One download as the windows see it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: u64,
    pub track_id: i64,
    pub url: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub status: DownloadStatus,
    /// The album or playlist it is downloaded with.
    pub group: Option<GroupInfo>,
    /// Stops the running yt-dlp.
    #[serde(skip)]
    cancel: Option<watch::Sender<bool>>,
    /// The folder and file name (without extension) this job reserved.
    #[serde(skip)]
    stem: Option<(PathBuf, String)>,
}

#[derive(Clone)]
pub struct Downloads(Arc<Inner>);

struct Inner {
    app: AppHandle,
    jobs: Mutex<Vec<Job>>,
    groups: Mutex<Vec<Arc<Group>>>,
    next_id: AtomicU64,
    /// Partial files, one directory per job.
    temp_root: PathBuf,
    last_progress: Mutex<Instant>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The folder downloads go to: the chosen one, else `Vitra` in the user's music folder.
pub fn download_folder(app: &AppHandle, settings: &DownloadSettings) -> Result<PathBuf, AppError> {
    if !settings.folder.trim().is_empty() {
        return Ok(PathBuf::from(settings.folder.trim()));
    }
    let music = app
        .path()
        .audio_dir()
        .or_else(|_| app.path().home_dir().map(|home| home.join("Music")))?;
    Ok(music.join("Vitra"))
}

/// File names (without extension, lowercased) in `dir`: a new download must not take one.
fn stems_in(dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            Path::new(&entry.file_name())
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_lowercase())
        })
        .collect()
}

/// What a download's file is tagged with.
#[derive(Debug, Clone, Default, PartialEq)]
struct Tagging {
    title: String,
    artists: Vec<String>,
    album: Option<String>,
    album_artist: Option<String>,
    year: Option<u16>,
    /// Number and total, for album tracks.
    track: Option<(u32, Option<u32>)>,
}

/// The tags of a track downloaded on its own, or as part of `album`.
fn tagging(track: &Track, album: Option<&LinkAlbum>, album_year: Option<i64>) -> Tagging {
    let artists = if track.artists.is_empty() {
        track.artist.clone().into_iter().collect()
    } else {
        track.artists.iter().map(|a| a.name.clone()).collect()
    };
    let year = track
        .year
        .or(album_year)
        .and_then(|year| u16::try_from(year).ok())
        .or_else(|| album.and_then(|a| a.year));
    Tagging {
        title: track.title.clone(),
        artists,
        album: album
            .map(|a| a.title.clone())
            .or_else(|| track.album.clone()),
        album_artist: album
            .and_then(|a| a.artist.clone())
            .or_else(|| track.album_artist.clone()),
        year,
        track: album.and_then(|a| {
            let number = track.track_number.and_then(|n| u32::try_from(n).ok())?;
            Some((number, a.count))
        }),
    }
}

/// The download's folder (under the download folder) and file name.
fn placement(
    base: &Path,
    settings: &DownloadSettings,
    tags: &Tagging,
    in_album: bool,
) -> (PathBuf, String) {
    let template = if in_album {
        &settings.album_template
    } else {
        &settings.template
    };
    let artist = tags.artists.join(", ");
    let (folders, stem) = links::relative_path(
        template,
        &FileNameFields {
            artist: (!artist.is_empty()).then_some(artist.as_str()),
            title: Some(&tags.title),
            album: tags.album.as_deref(),
            album_artist: tags.album_artist.as_deref(),
            track: tags.track.map(|(number, _)| number),
            year: tags.year,
        },
    );
    let dir = folders
        .iter()
        .fold(base.to_path_buf(), |dir, f| dir.join(f));
    (dir, stem)
}

impl Downloads {
    pub fn new(app: AppHandle) -> Result<Self, AppError> {
        let temp_root = app.path().app_cache_dir()?.join("downloads");
        // Partial files of downloads cut short by a crash or quitting.
        let _ = std::fs::remove_dir_all(&temp_root);
        Ok(Self(Arc::new(Inner {
            app,
            jobs: Mutex::new(Vec::new()),
            groups: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            temp_root,
            last_progress: Mutex::new(Instant::now()),
        })))
    }

    pub fn jobs(&self) -> Vec<Job> {
        lock(&self.0.jobs).clone()
    }

    fn emit(&self) {
        *lock(&self.0.last_progress) = Instant::now();
        if let Err(err) = self.0.app.emit(DOWNLOADS_EVENT, self.jobs()) {
            log::warn!("cannot emit {DOWNLOADS_EVENT}: {err}");
        }
    }

    fn emit_progress(&self) {
        if lock(&self.0.last_progress).elapsed() >= PROGRESS_INTERVAL {
            self.emit();
        }
    }

    /// Changes a job's status if the transition is allowed; returns whether it changed.
    fn transition(
        &self,
        id: u64,
        next: impl FnOnce(&DownloadStatus) -> Option<DownloadStatus>,
    ) -> bool {
        let mut jobs = lock(&self.0.jobs);
        let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
            return false;
        };
        match next(&job.status) {
            Some(status) if status != job.status => {
                job.status = status;
                true
            }
            _ => false,
        }
    }

    /// The group for `spec`: the one downloading the same link, or a new one.
    fn group(&self, spec: GroupSpec) -> Arc<Group> {
        let mut groups = lock(&self.0.groups);
        if let Some(index) = groups.iter().position(|g| g.info.url == spec.url) {
            let old = &groups[index];
            let cover = lock(&old.cover).clone().or(spec.cover);
            let group = Arc::new(Group {
                info: GroupInfo {
                    id: old.info.id,
                    kind: spec.kind,
                    url: spec.url,
                    title: spec.title,
                    artist: spec.artist,
                    cover: spec.cover_id.or(old.info.cover.clone()),
                },
                album: spec.album,
                cover: Mutex::new(cover),
                first_track: spec.first_track,
            });
            groups[index] = Arc::clone(&group);
            return group;
        }
        let group = Arc::new(Group {
            info: GroupInfo {
                id: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                kind: spec.kind,
                url: spec.url,
                title: spec.title,
                artist: spec.artist,
                cover: spec.cover_id,
            },
            album: spec.album,
            cover: Mutex::new(spec.cover),
            first_track: spec.first_track,
        });
        groups.push(Arc::clone(&group));
        group
    }

    fn group_by_id(&self, id: u64) -> Option<Arc<Group>> {
        lock(&self.0.groups)
            .iter()
            .find(|g| g.info.id == id)
            .cloned()
    }

    /// Queues link tracks for download, as `group` when given. Tracks that are files
    /// already, or queued already, are skipped. Returns how many were queued.
    pub fn add(&self, tracks: Vec<Track>, group: Option<GroupSpec>) -> usize {
        let group = group.map(|spec| self.group(spec));
        let mut added = 0;
        {
            let mut jobs = lock(&self.0.jobs);
            for track in tracks {
                let Some(url) = track.source_url.clone() else {
                    continue;
                };
                if !links::is_remote(&track.path) {
                    continue;
                }
                let active = jobs
                    .iter()
                    .any(|job| job.track_id == track.id && job.status.is_active());
                if active {
                    continue;
                }
                // A failed or cancelled attempt of the same track makes room for this one.
                jobs.retain(|job| job.track_id != track.id);
                jobs.push(Job {
                    id: self.0.next_id.fetch_add(1, Ordering::Relaxed),
                    track_id: track.id,
                    url,
                    title: track.title,
                    artist: track.artist,
                    album: track.album,
                    status: DownloadStatus::Queued,
                    group: group.as_ref().map(|g| g.info.clone()),
                    cancel: None,
                    stem: None,
                });
                added += 1;
            }
        }
        if added > 0 {
            self.emit();
            self.pump();
        }
        added
    }

    pub fn cancel(&self, id: u64) {
        let cancel = {
            let mut jobs = lock(&self.0.jobs);
            let Some(job) = jobs.iter_mut().find(|job| job.id == id) else {
                return;
            };
            if job.status == DownloadStatus::Queued {
                job.status = DownloadStatus::Cancelled;
                None
            } else {
                job.cancel.clone()
            }
        };
        match cancel {
            // The running job reports itself cancelled once yt-dlp is gone.
            Some(cancel) => {
                let _ = cancel.send(true);
            }
            None => self.emit(),
        }
    }

    pub fn retry(&self, id: u64) {
        if self.transition(id, DownloadStatus::retried) {
            self.emit();
            self.pump();
        }
    }

    fn group_job_ids(&self, group: u64) -> Vec<u64> {
        lock(&self.0.jobs)
            .iter()
            .filter(|job| job.group.as_ref().is_some_and(|g| g.id == group))
            .map(|job| job.id)
            .collect()
    }

    /// Cancels every unfinished download of an album or playlist.
    pub fn cancel_group(&self, group: u64) {
        for id in self.group_job_ids(group) {
            self.cancel(id);
        }
    }

    /// Retries every failed or cancelled download of an album or playlist.
    pub fn retry_group(&self, group: u64) {
        let mut retried = false;
        for id in self.group_job_ids(group) {
            retried |= self.transition(id, DownloadStatus::retried);
        }
        if retried {
            self.emit();
            self.pump();
        }
    }

    /// Forgets finished, failed and cancelled downloads (and groups left empty).
    pub fn clear(&self) {
        let groups: HashSet<u64> = {
            let mut jobs = lock(&self.0.jobs);
            jobs.retain(|job| job.status.is_active());
            jobs.iter()
                .filter_map(|job| job.group.as_ref().map(|g| g.id))
                .collect()
        };
        lock(&self.0.groups).retain(|g| groups.contains(&g.info.id));
        self.emit();
    }

    /// Starts queued jobs while there are free slots.
    fn pump(&self) {
        loop {
            let started = {
                let mut jobs = lock(&self.0.jobs);
                let running = jobs
                    .iter()
                    .filter(|job| job.status.is_active() && job.status != DownloadStatus::Queued)
                    .count();
                if running >= PARALLEL {
                    return;
                }
                let Some(job) = jobs
                    .iter_mut()
                    .find(|job| job.status == DownloadStatus::Queued)
                else {
                    return;
                };
                let Some(status) = job.status.started() else {
                    return;
                };
                job.status = status;
                let (sender, receiver) = watch::channel(false);
                job.cancel = Some(sender);
                (job.clone(), receiver)
            };
            self.emit();
            let (job, cancel) = started;
            let downloads = self.clone();
            tauri::async_runtime::spawn(async move {
                let id = job.id;
                let result = downloads.run(job, cancel).await;
                downloads.finish(id, result);
                downloads.pump();
            });
        }
    }

    fn finish(&self, id: u64, result: Result<String, AppError>) {
        {
            let mut jobs = lock(&self.0.jobs);
            if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
                job.cancel = None;
                job.stem = None;
                let next = match result {
                    Ok(path) => job.status.finished(path),
                    Err(AppError::Link(LinkError::Cancelled)) => job.status.cancelled(),
                    Err(err) => {
                        log::warn!("downloading {} failed: {err}", job.url);
                        job.status.failed(err.to_string())
                    }
                };
                if let Some(status) = next {
                    job.status = status;
                }
            }
        }
        let _ = std::fs::remove_dir_all(self.0.temp_root.join(id.to_string()));
        self.emit();
    }

    /// Reserves a file name no file in `dir` and no other download into `dir` has.
    fn reserve_stem(&self, id: u64, dir: &Path, wanted: &str) -> String {
        let on_disk = stems_in(dir);
        let mut jobs = lock(&self.0.jobs);
        let reserved: HashSet<String> = jobs
            .iter()
            .filter_map(|job| job.stem.as_ref())
            .filter(|(folder, _)| folder == dir)
            .map(|(_, stem)| stem.to_lowercase())
            .collect();
        let stem = links::unique_stem(wanted, |candidate| {
            let key = candidate.to_lowercase();
            on_disk.contains(&key) || reserved.contains(&key)
        });
        if let Some(job) = jobs.iter_mut().find(|job| job.id == id) {
            job.stem = Some((dir.to_path_buf(), stem.clone()));
        }
        stem
    }

    /// The album's cover for a track: the group's, else its first track's thumbnail, else
    /// the cover yt-dlp embedded in `file` (cut square). The first found is kept for the
    /// album's other tracks.
    async fn album_cover(
        &self,
        group: &Group,
        links: &LinkService,
        file: &Path,
    ) -> Option<Arc<Vec<u8>>> {
        if let Some(cover) = lock(&group.cover).clone() {
            return Some(cover);
        }
        let thumbnail = group
            .first_track
            .as_deref()
            .and_then(|url| links.thumbnail(url))
            .map(|bytes| bytes.as_ref().clone());
        let file = file.to_path_buf();
        let square = tauri::async_runtime::spawn_blocking(move || {
            let source = thumbnail.or_else(|| {
                media::read_track(&file)
                    .ok()?
                    .embedded_artwork
                    .map(|art| art.bytes)
            })?;
            media::square_cover(&source).ok()
        })
        .await
        .ok()
        .flatten()?;
        let square = Arc::new(square);
        let mut cover = lock(&group.cover);
        Some(Arc::clone(cover.get_or_insert(square)))
    }

    /// Downloads one job's link, tags the file and puts it in the library; returns the file.
    async fn run(&self, job: Job, cancel: watch::Receiver<bool>) -> Result<String, AppError> {
        let app = &self.0.app;
        let links = app.state::<LinkService>().inner().clone();
        let library = app.state::<LibraryService>().inner().clone();
        let settings = app.state::<SettingsStore>().get().downloads;

        let ytdlp = links.ytdlp()?;
        if ytdlp.ffmpeg.is_none() {
            return Err(AppError::NotFound(
                "FFmpeg is needed for downloads but was not found".into(),
            ));
        }
        let group = job.group.as_ref().and_then(|g| self.group_by_id(g.id));
        let album = group.as_ref().and_then(|g| g.album.clone());
        let tags = self.tagging(&library, &job, album.as_ref()).await?;
        let base = download_folder(app, &settings)?;
        let (dir, wanted) = placement(&base, &settings, &tags, album.is_some());
        let options = DownloadOptions {
            format: settings.format,
            mp3_bitrate: settings.mp3_bitrate,
            file_stem: self.reserve_stem(job.id, &dir, &wanted),
            dir,
            temp_dir: self.0.temp_root.join(job.id.to_string()),
            ffmpeg: ytdlp.ffmpeg.clone(),
        };

        let id = job.id;
        let file = ytdlp
            .download(&job.url, &options, cancel, |event| {
                if self.transition(id, |status| status.on_event(event)) {
                    self.emit();
                } else if matches!(event, DownloadEvent::Progress { .. }) {
                    self.emit_progress();
                }
            })
            .await?;
        // Tagging and moving into the library are part of processing.
        if self.transition(id, |status| {
            status.on_event(&DownloadEvent::Processing("Vitra".into()))
        }) {
            self.emit();
        }

        // Read again: the album's other tracks may have brought its year meanwhile.
        let tags = self.tagging(&library, &job, album.as_ref()).await?;
        let cover = match (&group, &album) {
            (Some(group), Some(_)) => self.album_cover(group, &links, &file).await,
            _ => None,
        };
        let track = {
            let library = library.clone();
            let track_id = job.track_id;
            let file = file.clone();
            tauri::async_runtime::spawn_blocking(move || {
                tag_file(&library, track_id, &tags, cover.as_deref(), &file);
                library.with(|db| db.attach_download(track_id, &file, library.covers_dir()))
            })
            .await??
        };
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        library.changed(Change::Folders);
        if let Some(player) = app.try_state::<Player>() {
            player.link_downloaded(&job.url, &track);
        }
        Ok(track.path)
    }

    /// The tags for a job, from the library's current view of its track (filled in while the
    /// download waited) and its album.
    async fn tagging(
        &self,
        library: &LibraryService,
        job: &Job,
        album: Option<&LinkAlbum>,
    ) -> Result<Tagging, AppError> {
        let track_id = job.track_id;
        let stored = library
            .run(move |db| {
                let Some(track) = db.track(track_id)? else {
                    return Ok(None);
                };
                let album_year = match track.album_id {
                    Some(id) => db.album(id)?.and_then(|a| a.year),
                    None => None,
                };
                Ok(Some((track, album_year)))
            })
            .await?;
        Ok(match stored {
            Some((track, album_year)) => tagging(&track, album, album_year),
            // The track left the library meanwhile: what the job knew.
            None => Tagging {
                title: job.title.clone(),
                artists: job.artist.clone().into_iter().collect(),
                album: job.album.clone(),
                ..Tagging::default()
            },
        })
    }
}

/// Writes the track's metadata as Vitra shows it (cleaner than the page's title and
/// uploader yt-dlp embeds), each artist as its own value, and its lyrics into the file.
/// Values Vitra doesn't know (a year, an album artist) keep what yt-dlp wrote; `cover`
/// replaces the thumbnail yt-dlp embedded.
fn tag_file(
    library: &LibraryService,
    track_id: i64,
    tags: &Tagging,
    cover: Option<&Vec<u8>>,
    file: &Path,
) {
    let lyrics = library.with(|db| db.lyrics_text(track_id)).ok().flatten();
    let written = media::read_track(file).ok().map(|info| info.metadata);
    let values = media::TagValues {
        title: tags.title.clone(),
        artist: tags.artists.join(&media::ARTIST_SEPARATOR.to_string()),
        album: tags.album.clone().unwrap_or_default(),
        album_artist: tags
            .album_artist
            .clone()
            .or_else(|| written.as_ref().and_then(|m| m.album_artist.clone()))
            .unwrap_or_default(),
        year: tags.year.or_else(|| written.as_ref().and_then(|m| m.year)),
        track: tags.track,
        lyrics,
    };
    let cover = match cover {
        Some(bytes) => media::CoverChange::Replace(bytes.clone()),
        None => media::CoverChange::Keep,
    };
    if let Err(err) = media::write_tags(file, &values, &cover) {
        log::warn!("cannot tag {}: {err}", file.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use library::ArtistRef;

    #[test]
    fn existing_files_are_found_by_stem_ignoring_case() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Artist - Song.opus"), b"").unwrap();
        std::fs::write(tmp.path().join("Other.mp3"), b"").unwrap();
        let stems = stems_in(tmp.path());
        assert!(stems.contains("artist - song"));
        assert!(stems.contains("other"));
        assert!(stems_in(&tmp.path().join("missing")).is_empty());
    }

    fn track() -> Track {
        Track {
            id: 1,
            path: "https://y/1".into(),
            title: "Ice Fields".into(),
            artist: Some("Glass Harbor, Aurora Collective".into()),
            artist_id: Some(2),
            artists: vec![
                ArtistRef {
                    id: 2,
                    name: "Glass Harbor".into(),
                },
                ArtistRef {
                    id: 1,
                    name: "Aurora Collective".into(),
                },
            ],
            album: Some("Polar Lights".into()),
            album_id: Some(1),
            album_artist: Some("Aurora Collective".into()),
            genre: None,
            year: None,
            track_number: Some(2),
            disc_number: None,
            duration: Some(187.0),
            format: None,
            sample_rate: None,
            bit_depth: None,
            bitrate: None,
            favorite: false,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            cover: None,
            folder_id: None,
            source_url: Some("https://y/1".into()),
        }
    }

    #[test]
    fn album_tracks_are_numbered_into_album_folders() {
        let album = LinkAlbum {
            title: "Polar Lights".into(),
            artist: Some("Aurora Collective".into()),
            year: None,
            count: Some(12),
        };
        let tags = tagging(&track(), Some(&album), Some(2021));
        assert_eq!(tags.artists, ["Glass Harbor", "Aurora Collective"]);
        assert_eq!(tags.track, Some((2, Some(12))));
        assert_eq!(tags.year, Some(2021));
        assert_eq!(tags.album_artist.as_deref(), Some("Aurora Collective"));

        let settings = DownloadSettings::default();
        let base = Path::new("/music/Vitra");
        let (dir, stem) = placement(base, &settings, &tags, true);
        assert_eq!(dir, base.join("Aurora Collective").join("Polar Lights"));
        assert_eq!(stem, "02 Ice Fields");
        // On its own, the same track goes by the single-track template.
        let single = tagging(&track(), None, None);
        assert_eq!(single.track, None);
        let (dir, stem) = placement(base, &settings, &single, false);
        assert_eq!(dir, base);
        assert_eq!(stem, "Glass Harbor, Aurora Collective - Ice Fields");
    }
}
