//! Folder scanning: lists audio files, reads tags of new and changed ones in parallel,
//! drops files that are gone, follows files that were moved or renamed, and caches album
//! covers as small JPEGs.
//!
//! Scans are incremental: a file whose size and modification time match the database is not
//! read again, so rescanning a large, unchanged library only costs a directory walk.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use media::TrackMetadata;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::folders::KnownFile;
use crate::model::{ScannedTrack, Track};
use crate::tracks::{delete_tracks, file_title, prune, upsert_track};
use crate::{Error, Library, Result, now};

/// Extensions the scanner picks up. Video containers (`.mp4`, `.webm`) are left out so music
/// videos lying next to albums don't turn into tracks; they can still be opened directly.
pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "m4b", "aac", "alac", "ogg", "oga", "opus", "wav", "aif", "aiff", "aifc",
    "ape", "wv", "wma", "dsf", "dff", "mka", "mpc", "tta", "caf", "spx",
];

/// Longer side of cached album covers: sharp in a grid tile on a 2x display.
const COVER_SIZE: u32 = 512;
/// Tracks written per transaction: big enough to be fast, small enough that the UI sees
/// the library fill up during a long first scan.
const BATCH: usize = 250;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
}

/// A stable id for cover bytes (64-bit FNV-1a as hex), used as the cached file name. The
/// same image shared by several albums is stored once, and a changed cover gets a new URL,
/// so the WebView never shows a stale cached copy.
pub fn cover_id(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanPhase {
    Listing,
    Reading,
    Covers,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    pub folder_id: i64,
    pub phase: ScanPhase,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub added: usize,
    pub updated: usize,
    /// Files found under a new path (moved or renamed); they keep their history.
    pub moved: usize,
    pub removed: usize,
    pub unchanged: usize,
    pub cancelled: bool,
}

impl ScanSummary {
    pub fn changed(&self) -> bool {
        self.added + self.updated + self.moved + self.removed > 0
    }
}

struct FoundFile {
    path: PathBuf,
    /// `path` as stored in the database.
    key: String,
    size: u64,
    mtime: i64,
}

struct Listing {
    files: Vec<FoundFile>,
    /// Directories that could not be read: their known files are kept, not deleted.
    unreadable: Vec<PathBuf>,
}

pub(crate) fn mtime(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64)
}

fn is_hidden(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| name.starts_with('.'))
}

/// Lists audio files under `root`. Fails only if `root` itself can't be read (e.g. an
/// unplugged drive), so an offline folder never looks empty and loses its tracks.
fn list(root: &Path, cancel: &AtomicBool) -> std::io::Result<Listing> {
    let mut listing = Listing {
        files: Vec::new(),
        unreadable: Vec::new(),
    };
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if dir == root => return Err(err),
            Err(err) => {
                log::warn!("cannot read {}: {err}", dir.display());
                listing.unreadable.push(dir);
                continue;
            }
        };
        for entry in entries {
            let Ok(entry) = entry else {
                listing.unreadable.push(dir.clone());
                continue;
            };
            if is_hidden(&entry.file_name()) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            // Symlinked directories are not followed, so a link loop can't trap the walk;
            // symlinked files are.
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !is_audio_file(&path) {
                continue;
            }
            let Some(key) = path.to_str().map(str::to_owned) else {
                log::warn!("skipping a file with a non-UTF-8 path: {}", path.display());
                continue;
            };
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            if metadata.is_file() {
                listing.files.push(FoundFile {
                    key,
                    size: metadata.len(),
                    mtime: mtime(&metadata),
                    path,
                });
            }
        }
    }
    Ok(listing)
}

/// Tags of a file; files lofty can't parse (DSF, WMA, ...) are kept with what the path
/// tells, since mpv still plays them. A parser panic on a damaged file counts as "no tags"
/// rather than taking the whole scan down.
pub(crate) fn read_metadata(path: &Path) -> TrackMetadata {
    match std::panic::catch_unwind(|| media::read_track(path)) {
        Ok(Ok(info)) => {
            let mut metadata = info.metadata;
            // Lyrics in a `.lrc` next to the file are searchable too.
            if metadata.lyrics.is_none() {
                metadata.lyrics = lyrics::read_lrc(path);
            }
            return metadata;
        }
        Ok(Err(err)) => log::debug!("no tags for {}: {err}", path.display()),
        Err(_) => log::warn!("reading tags of {} panicked", path.display()),
    }
    TrackMetadata {
        format: path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_uppercase),
        ..TrackMetadata::default()
    }
}

/// Runs `work` over `items` on a few threads and hands each result to `sink` on the calling
/// thread (which owns the database connection). Stops early when `cancel` is set.
fn parallel<T: Sync, R: Send>(
    items: &[T],
    cancel: &AtomicBool,
    work: impl Fn(&T) -> R + Sync,
    mut sink: impl FnMut(usize, R) -> Result<()>,
) -> Result<()> {
    let threads = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(2, 8)
        .min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::sync_channel::<(usize, R)>(64);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let sender = sender.clone();
            let (next, work) = (&next, &work);
            scope.spawn(move || {
                while !cancel.load(Ordering::Relaxed) {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    if sender.send((index, work(item))).is_err() {
                        break;
                    }
                }
            });
        }
        drop(sender);
        for (index, result) in receiver {
            if let Err(err) = sink(index, result) {
                // Dropping the receiver stops the workers.
                cancel.store(true, Ordering::Relaxed);
                return Err(err);
            }
        }
        Ok(())
    })
}

/// Rate-limits progress reports.
struct Reporter<'a> {
    folder_id: i64,
    callback: &'a mut dyn FnMut(ScanProgress),
    last: Option<Instant>,
}

impl Reporter<'_> {
    fn report(&mut self, phase: ScanPhase, done: usize, total: usize) {
        let due = self
            .last
            .is_none_or(|last| last.elapsed() >= PROGRESS_INTERVAL);
        if due || done == total {
            self.last = Some(Instant::now());
            (self.callback)(ScanProgress {
                folder_id: self.folder_id,
                phase,
                done,
                total,
            });
        }
    }
}

/// Finds an album's cover: embedded in one of its first tracks, or an image next to them.
fn find_cover(paths: &[String]) -> Option<Vec<u8>> {
    paths
        .iter()
        .take(3)
        .find_map(|path| {
            std::panic::catch_unwind(|| media::read_track(Path::new(path)))
                .ok()?
                .ok()?
                .embedded_artwork
        })
        .or_else(|| {
            paths
                .first()
                .and_then(|path| media::folder_artwork(Path::new(path)))
        })
        .map(|artwork| artwork.bytes)
}

/// How [`Library::store_files`] treats files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Store {
    /// Re-read files the library knows (or a library folder contains); ignore others.
    Refresh,
    /// Add any file; skip files the library already has unchanged.
    Add,
}

/// Temporary cover files older than this are leftovers of a crash, not a write in progress.
const STALE_PART: Duration = Duration::from_secs(600);

/// Writes the cover thumbnail if it isn't cached yet; returns its id.
pub(crate) fn cache_cover(bytes: &[u8], covers_dir: &Path) -> Option<String> {
    // Albums sharing a cover get the same id and may be cached by two threads at once, so
    // each write uses its own temporary file.
    static NEXT_PART: AtomicU64 = AtomicU64::new(0);
    let id = cover_id(bytes);
    let target = covers_dir.join(format!("{id}.jpg"));
    if target.is_file() {
        return Some(id);
    }
    let thumbnail = media::cover_thumbnail(bytes, COVER_SIZE)
        .inspect_err(|err| log::info!("cannot decode a cover: {err}"))
        .ok()?;
    // Written under a temporary name first, so a half-written file is never served.
    let part = NEXT_PART.fetch_add(1, Ordering::Relaxed);
    let partial = covers_dir.join(format!("{id}.{}-{part}.part", std::process::id()));
    let written = fs::create_dir_all(covers_dir)
        .and_then(|()| fs::write(&partial, thumbnail))
        .and_then(|()| fs::rename(&partial, &target));
    match written {
        Ok(()) => Some(id),
        // Another thread finished the same cover first.
        Err(_) if target.is_file() => {
            let _ = fs::remove_file(&partial);
            Some(id)
        }
        Err(err) => {
            log::warn!("cannot cache a cover in {}: {err}", covers_dir.display());
            let _ = fs::remove_file(&partial);
            None
        }
    }
}

impl Library {
    /// Brings a library folder up to date with the disk. `progress` is called a few times per
    /// second; setting `cancel` stops the scan after the current batch (nothing is deleted
    /// then, since the listing may be incomplete).
    pub fn scan_folder(
        &mut self,
        folder_id: i64,
        covers_dir: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(ScanProgress),
    ) -> Result<ScanSummary> {
        let folder = self
            .folder(folder_id)?
            .ok_or_else(|| Error::Invalid(format!("no folder {folder_id}")))?;
        let mut reporter = Reporter {
            folder_id,
            callback: progress,
            last: None,
        };
        let mut summary = ScanSummary::default();

        reporter.report(ScanPhase::Listing, 0, 0);
        let listing = list(Path::new(&folder.path), cancel)?;
        if cancel.load(Ordering::Relaxed) {
            summary.cancelled = true;
            return Ok(summary);
        }

        let mut known = self.known_files(folder_id)?;
        // A folder's first scan dates tracks by their files, so "Recently added" shows what
        // really is new rather than the whole collection at once.
        let first_scan = known.is_empty();
        let scan_time = now();
        let mut to_read = Vec::new();
        for file in listing.files {
            match known.remove(&file.key) {
                Some(k) if k.size == file.size as i64 && k.mtime == file.mtime => {
                    summary.unchanged += 1;
                }
                Some(_) => to_read.push((file, true)),
                None => to_read.push((file, false)),
            }
        }
        // What's left in `known` wasn't found on disk.
        let mut missing: Vec<(String, KnownFile)> = known
            .into_iter()
            .filter(|(path, _)| {
                !listing
                    .unreadable
                    .iter()
                    .any(|dir| Path::new(path).starts_with(dir))
            })
            .collect();

        let total = to_read.len();
        let mut done = 0;
        let mut batch: Vec<(ScannedTrack, bool)> = Vec::with_capacity(BATCH);
        let mut touched_albums = HashSet::new();
        reporter.report(ScanPhase::Reading, 0, total);
        parallel(
            &to_read,
            cancel,
            |(file, _)| read_metadata(&file.path),
            |index, metadata| {
                let (file, known) = &to_read[index];
                batch.push((
                    ScannedTrack {
                        path: file.key.clone(),
                        size: file.size,
                        mtime: file.mtime,
                        metadata,
                    },
                    *known,
                ));
                done += 1;
                if batch.len() >= BATCH {
                    self.store_batch(
                        folder_id,
                        &mut batch,
                        &mut missing,
                        first_scan.then_some(scan_time),
                        scan_time,
                        &mut summary,
                        &mut touched_albums,
                    )?;
                }
                reporter.report(ScanPhase::Reading, done, total);
                Ok(())
            },
        )?;
        self.store_batch(
            folder_id,
            &mut batch,
            &mut missing,
            first_scan.then_some(scan_time),
            scan_time,
            &mut summary,
            &mut touched_albums,
        )?;

        if cancel.load(Ordering::Relaxed) {
            summary.cancelled = true;
        } else {
            let ids: Vec<i64> = missing.iter().map(|(_, file)| file.id).collect();
            summary.removed = ids.len();
            let tx = self.conn.transaction()?;
            delete_tracks(&tx, &ids)?;
            prune(&tx)?;
            tx.commit()?;
        }

        let mut albums: Vec<i64> = touched_albums.into_iter().collect();
        albums.sort_unstable();
        self.update_covers(&albums, covers_dir, cancel, &mut reporter)?;
        if !summary.cancelled {
            self.remove_unused_covers(covers_dir)?;
        }
        log::info!("scanned {}: {summary:?}", folder.path);
        Ok(summary)
    }

    /// Writes a batch of scanned files. New files that match a missing one (same size and
    /// title) take over its row, so a moved or renamed file keeps its play count, favorite
    /// flag and playlist entries. `dated` (first scan) dates new tracks by file time.
    #[allow(clippy::too_many_arguments)]
    fn store_batch(
        &mut self,
        folder_id: i64,
        batch: &mut Vec<(ScannedTrack, bool)>,
        missing: &mut Vec<(String, KnownFile)>,
        dated: Option<i64>,
        scan_time: i64,
        summary: &mut ScanSummary,
        touched_albums: &mut HashSet<i64>,
    ) -> Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        let splitting = crate::artists::splitting(&tx)?;
        for (track, known) in batch.drain(..) {
            if known {
                summary.updated += 1;
            } else {
                let title = track
                    .metadata
                    .title
                    .clone()
                    .unwrap_or_else(|| file_title(&track.path));
                let moved = missing
                    .iter()
                    .position(|(_, file)| file.size == track.size as i64 && file.title == title);
                if let Some(index) = moved {
                    let (_, file) = missing.swap_remove(index);
                    tx.execute(
                        "UPDATE tracks SET path = ?2 WHERE id = ?1",
                        params![file.id, track.path],
                    )?;
                    summary.moved += 1;
                } else {
                    summary.added += 1;
                }
            }
            let added_at = dated.map_or(scan_time, |now| track.mtime.clamp(0, now));
            let id = upsert_track(&tx, Some(folder_id), &track, added_at, &splitting)?;
            let album: Option<i64> = tx
                .query_row("SELECT album_id FROM tracks WHERE id = ?", [id], |row| {
                    row.get(0)
                })
                .optional()?
                .flatten();
            touched_albums.extend(album);
        }
        // Retagging can leave an album or artist empty.
        prune(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Re-reads tags of specific files, e.g. after the tag editor saved them. Files that
    /// aren't in the library (or inside a library folder) are ignored.
    pub fn refresh_files(&mut self, paths: &[&Path], covers_dir: &Path) -> Result<()> {
        self.store_files(paths, covers_dir, Store::Refresh)
    }

    /// Adds files the user opened or dropped: files inside a library folder join it, others
    /// become single tracks. Files already in the library are only re-read if they changed.
    /// Returns the library tracks of the given paths, in order.
    pub fn add_files(&mut self, paths: &[&Path], covers_dir: &Path) -> Result<Vec<Track>> {
        self.store_files(paths, covers_dir, Store::Add)?;
        let mut tracks = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some(key) = path.to_str() {
                tracks.extend(self.track_by_path(key)?);
            }
        }
        Ok(tracks)
    }

    /// Reads tags of files outside a folder scan and stores them, with album covers.
    fn store_files(&mut self, paths: &[&Path], covers_dir: &Path, mode: Store) -> Result<()> {
        struct Job {
            path: PathBuf,
            key: String,
            folder_id: Option<i64>,
            size: u64,
            mtime: i64,
        }
        let mut touched_albums = HashSet::new();
        let mut jobs = Vec::new();
        for path in paths {
            let Some(key) = path.to_str() else {
                continue;
            };
            let Ok(metadata) = fs::metadata(path) else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            let (size, mtime) = (metadata.len(), mtime(&metadata));
            let existing: Option<(Option<i64>, Option<i64>, i64, i64)> = self
                .conn
                .query_row(
                    "SELECT folder_id, album_id, file_size, file_mtime FROM tracks WHERE path = ?",
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()?;
            let folder_id = match existing {
                Some((_, _, known_size, known_mtime))
                    if mode == Store::Add && known_size == size as i64 && known_mtime == mtime =>
                {
                    continue;
                }
                Some((folder_id, album_id, ..)) => {
                    // The old album may lose its cover source.
                    touched_albums.extend(album_id);
                    folder_id
                }
                None => match self.folder_containing(path)? {
                    Some(folder) => Some(folder.id),
                    None if mode == Store::Add => None,
                    None => continue,
                },
            };
            jobs.push(Job {
                path: path.to_path_buf(),
                key: key.to_owned(),
                folder_id,
                size,
                mtime,
            });
        }
        if jobs.is_empty() {
            return Ok(());
        }

        let mut read: Vec<Option<TrackMetadata>> = vec![None; jobs.len()];
        parallel(
            &jobs,
            &AtomicBool::new(false),
            |job| read_metadata(&job.path),
            |index, metadata| {
                read[index] = Some(metadata);
                Ok(())
            },
        )?;

        let tx = self.conn.transaction()?;
        let at = now();
        let splitting = crate::artists::splitting(&tx)?;
        for (job, metadata) in jobs.iter().zip(read) {
            let track = ScannedTrack {
                path: job.key.clone(),
                size: job.size,
                mtime: job.mtime,
                metadata: metadata.unwrap_or_default(),
            };
            let id = upsert_track(&tx, job.folder_id, &track, at, &splitting)?;
            let album: Option<i64> =
                tx.query_row("SELECT album_id FROM tracks WHERE id = ?", [id], |row| {
                    row.get(0)
                })?;
            touched_albums.extend(album);
        }
        prune(&tx)?;
        tx.commit()?;

        let mut albums: Vec<i64> = touched_albums.into_iter().collect();
        albums.sort_unstable();
        self.refresh_covers(&albums, covers_dir)
    }

    /// Finds the covers of albums whose tracks changed outside a scan and drops unused ones.
    pub(crate) fn refresh_covers(&mut self, album_ids: &[i64], covers_dir: &Path) -> Result<()> {
        let mut ignore = |_| {};
        let mut reporter = Reporter {
            folder_id: 0,
            callback: &mut ignore,
            last: None,
        };
        self.update_covers(
            album_ids,
            covers_dir,
            &AtomicBool::new(false),
            &mut reporter,
        )?;
        self.remove_unused_covers(covers_dir)?;
        Ok(())
    }

    /// Finds and caches covers for the given albums (in parallel: decoding is the slow part).
    fn update_covers(
        &mut self,
        album_ids: &[i64],
        covers_dir: &Path,
        cancel: &AtomicBool,
        reporter: &mut Reporter<'_>,
    ) -> Result<()> {
        // Albums may have been pruned since they were touched.
        let mut albums = Vec::with_capacity(album_ids.len());
        for &id in album_ids {
            let paths = self.album_paths(id)?;
            if !paths.is_empty() {
                albums.push((id, paths));
            }
        }
        let total = albums.len();
        let mut done = 0;
        let mut covers: HashMap<i64, Option<String>> = HashMap::new();
        reporter.report(ScanPhase::Covers, 0, total);
        parallel(
            &albums,
            cancel,
            |(_, paths)| find_cover(paths).and_then(|bytes| cache_cover(&bytes, covers_dir)),
            |index, cover| {
                covers.insert(albums[index].0, cover);
                done += 1;
                reporter.report(ScanPhase::Covers, done, total);
                Ok(())
            },
        )?;
        let tx = self.conn.transaction()?;
        for (album, cover) in &covers {
            tx.execute(
                "UPDATE albums SET cover = ?2 WHERE id = ?1",
                params![album, cover],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Deletes cached covers no album uses any more; returns how many.
    pub fn remove_unused_covers(&self, covers_dir: &Path) -> Result<usize> {
        let Ok(entries) = fs::read_dir(covers_dir) else {
            return Ok(0);
        };
        let used = self.cover_ids()?;
        let mut removed = 0;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let unused = match name.strip_suffix(".jpg") {
                Some(id) => !used.contains(id),
                // A temporary file is someone's write in progress unless it is old.
                None => {
                    name.ends_with(".part")
                        && entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .ok()
                            .and_then(|t| t.elapsed().ok())
                            .is_some_and(|age| age > STALE_PART)
                }
            };
            if unused && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackFilter;
    use crate::tracks::AlbumSort;

    fn fixtures() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures")
    }

    struct Setup {
        _tmp: tempfile::TempDir,
        music: PathBuf,
        covers: PathBuf,
        library: Library,
        folder: i64,
    }

    /// A library folder holding copies of the media crate's fixtures in two albums.
    fn setup() -> Setup {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("Music");
        fs::create_dir_all(music.join("Fixtures")).unwrap();
        fs::create_dir_all(music.join("Other/.hidden")).unwrap();
        fs::copy(
            fixtures().join("tagged.flac"),
            music.join("Fixtures/03 Aurora.flac"),
        )
        .unwrap();
        fs::copy(fixtures().join("tagged.mp3"), music.join("Other/polar.mp3")).unwrap();
        fs::copy(
            fixtures().join("untagged.wav"),
            music.join("Other/Untitled Take.wav"),
        )
        .unwrap();
        fs::copy(
            fixtures().join("untagged.wav"),
            music.join("Other/.hidden/skip.wav"),
        )
        .unwrap();
        fs::write(music.join("Other/notes.txt"), "not audio").unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let folder = library.add_folder(music.to_str().unwrap()).unwrap().id;
        Setup {
            covers: tmp.path().join("covers"),
            _tmp: tmp,
            music,
            library,
            folder,
        }
    }

    impl Setup {
        fn scan(&mut self) -> ScanSummary {
            let mut events = Vec::new();
            let summary = self
                .library
                .scan_folder(
                    self.folder,
                    &self.covers,
                    &AtomicBool::new(false),
                    &mut |p| events.push(p),
                )
                .unwrap();
            assert!(!events.is_empty());
            summary
        }

        fn titles(&self) -> Vec<String> {
            let mut titles: Vec<_> = self
                .library
                .tracks(&TrackFilter::All)
                .unwrap()
                .into_iter()
                .map(|t| t.title)
                .collect();
            titles.sort();
            titles
        }

        fn path(&self, relative: &str) -> String {
            self.music.join(relative).to_str().unwrap().to_owned()
        }
    }

    #[test]
    fn recognizes_audio_files() {
        assert!(is_audio_file(Path::new("/a/B.FLAC")));
        assert!(is_audio_file(Path::new("x.opus")));
        assert!(!is_audio_file(Path::new("clip.mp4")));
        assert!(!is_audio_file(Path::new("cover.jpg")));
        assert!(!is_audio_file(Path::new("flac")));
    }

    #[test]
    fn cover_ids_are_stable_hex() {
        assert_eq!(cover_id(b""), "cbf29ce484222325");
        assert_eq!(cover_id(b"a"), "af63dc4c8601ec8c");
        assert_ne!(cover_id(b"ab"), cover_id(b"ba"));
    }

    #[test]
    fn first_scan_adds_tagged_and_untagged_files() {
        let mut s = setup();
        let summary = s.scan();
        assert_eq!(summary.added, 3);
        assert_eq!(s.titles(), ["Aurora", "Polar Night", "Untitled Take"]);

        let aurora = s
            .library
            .track_by_path(&s.path("Fixtures/03 Aurora.flac"))
            .unwrap()
            .unwrap();
        assert_eq!(aurora.album.as_deref(), Some("Fixtures"));
        assert_eq!(aurora.track_number, Some(3));
        assert_eq!(aurora.format.as_deref(), Some("FLAC"));
        // Dated by the file, not the scan.
        let file_time = mtime(&fs::metadata(&aurora.path).unwrap());
        assert_eq!(aurora.added_at, file_time);
        assert_eq!(s.library.folders().unwrap()[0].track_count, 3);
    }

    #[test]
    fn caches_album_covers() {
        let mut s = setup();
        s.scan();
        let albums = s.library.albums(AlbumSort::Artist).unwrap();
        assert!(!albums.is_empty());
        for album in &albums {
            let cover = album.cover.as_ref().expect("fixture albums have artwork");
            let file = s.covers.join(format!("{cover}.jpg"));
            let bytes = fs::read(file).unwrap();
            assert!(bytes.starts_with(&[0xFF, 0xD8]), "a JPEG");
        }
        let track = s.library.tracks(&TrackFilter::All).unwrap();
        assert!(track.iter().any(|t| t.cover.is_some()));
    }

    #[test]
    fn caching_one_cover_from_many_threads_at_once_works() {
        // Albums sharing a cover race to cache the same id.
        let art = media::read_track(&fixtures().join("tagged.flac"))
            .unwrap()
            .embedded_artwork
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let ids: Vec<Option<String>> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..8)
                .map(|_| scope.spawn(|| cache_cover(&art.bytes, dir.path())))
                .collect();
            workers.into_iter().map(|w| w.join().unwrap()).collect()
        });
        let expected = cover_id(&art.bytes);
        assert!(
            ids.iter()
                .all(|id| id.as_deref() == Some(expected.as_str()))
        );
        let files: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            files,
            [format!("{expected}.jpg")],
            "no temporary files left"
        );
    }

    #[test]
    fn rescanning_an_unchanged_folder_reads_nothing() {
        let mut s = setup();
        s.scan();
        let summary = s.scan();
        assert_eq!(
            summary,
            ScanSummary {
                unchanged: 3,
                ..ScanSummary::default()
            }
        );
    }

    #[test]
    fn picks_up_changes_deletions_and_new_files() {
        let mut s = setup();
        s.scan();
        let before = s
            .library
            .track_by_path(&s.path("Other/polar.mp3"))
            .unwrap()
            .unwrap();

        fs::remove_file(s.music.join("Other/Untitled Take.wav")).unwrap();
        fs::copy(
            fixtures().join("untagged.wav"),
            s.music.join("New Take.wav"),
        )
        .unwrap();
        // A changed file: different size.
        let mut bytes = fs::read(s.music.join("Other/polar.mp3")).unwrap();
        bytes.extend_from_slice(&[0; 128]);
        fs::write(s.music.join("Other/polar.mp3"), bytes).unwrap();

        let summary = s.scan();
        assert_eq!(summary.added, 1);
        assert_eq!(summary.updated, 1);
        assert_eq!(summary.removed, 1);
        assert_eq!(summary.unchanged, 1);
        assert_eq!(s.titles(), ["Aurora", "New Take", "Polar Night"]);
        // The changed file kept its row; a later scan dates new files by the scan.
        let after = s
            .library
            .track_by_path(&s.path("Other/polar.mp3"))
            .unwrap()
            .unwrap();
        assert_eq!(after.id, before.id);
        let new = s
            .library
            .track_by_path(&s.path("New Take.wav"))
            .unwrap()
            .unwrap();
        assert!(new.added_at >= now() - 60);
    }

    #[test]
    fn moved_files_keep_their_history() {
        let mut s = setup();
        s.scan();
        let old = s
            .library
            .track_by_path(&s.path("Fixtures/03 Aurora.flac"))
            .unwrap()
            .unwrap();
        s.library.set_favorite(old.id, true).unwrap();
        s.library.record_play(old.id, 100).unwrap();

        fs::create_dir_all(s.music.join("Sorted")).unwrap();
        fs::rename(
            s.music.join("Fixtures/03 Aurora.flac"),
            s.music.join("Sorted/Aurora.flac"),
        )
        .unwrap();
        let summary = s.scan();
        assert_eq!((summary.moved, summary.added, summary.removed), (1, 0, 0));

        let moved = s
            .library
            .track_by_path(&s.path("Sorted/Aurora.flac"))
            .unwrap()
            .unwrap();
        assert_eq!(moved.id, old.id);
        assert!(moved.favorite);
        assert_eq!(moved.play_count, 1);
        assert_eq!(s.library.search("aurora", 10).unwrap().tracks.len(), 1);
    }

    #[test]
    fn an_unreachable_folder_keeps_its_tracks() {
        let mut s = setup();
        s.scan();
        fs::rename(&s.music, s.music.with_file_name("Unplugged")).unwrap();
        let result =
            s.library
                .scan_folder(s.folder, &s.covers, &AtomicBool::new(false), &mut |_| {});
        assert!(matches!(result, Err(Error::Io(_))));
        assert_eq!(s.titles().len(), 3);
    }

    #[test]
    fn a_cancelled_scan_deletes_nothing() {
        let mut s = setup();
        s.scan();
        fs::remove_file(s.music.join("Other/polar.mp3")).unwrap();
        let summary = s
            .library
            .scan_folder(s.folder, &s.covers, &AtomicBool::new(true), &mut |_| {})
            .unwrap();
        assert!(summary.cancelled);
        assert_eq!(s.titles().len(), 3);
    }

    #[test]
    fn removing_a_folder_lets_its_covers_go() {
        let mut s = setup();
        s.scan();
        let cached = || fs::read_dir(&s.covers).unwrap().count();
        assert!(cached() > 0);
        s.library.remove_folder(s.folder).unwrap();
        s.library.remove_unused_covers(&s.covers).unwrap();
        assert_eq!(cached(), 0);
    }

    #[test]
    fn opened_files_become_single_tracks_with_covers() {
        let mut s = setup();
        let elsewhere = s._tmp.path().join("Downloads");
        fs::create_dir_all(&elsewhere).unwrap();
        let file = elsewhere.join("song.flac");
        fs::copy(fixtures().join("tagged.flac"), &file).unwrap();

        let tracks = s.library.add_files(&[&file], &s.covers).unwrap();
        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.title, "Aurora");
        assert_eq!(track.folder_id, None);
        let cover = track
            .cover
            .as_ref()
            .expect("its album got the embedded cover");
        assert!(s.covers.join(format!("{cover}.jpg")).is_file());

        // Opening it again changes nothing.
        s.library.set_favorite(track.id, true).unwrap();
        let again = s.library.add_files(&[&file], &s.covers).unwrap();
        assert_eq!(again[0].id, track.id);
        assert!(again[0].favorite);

        // Missing files and folders are skipped.
        let none = s
            .library
            .add_files(&[&elsewhere, &elsewhere.join("gone.mp3")], &s.covers)
            .unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn opened_files_inside_a_library_folder_join_it() {
        let mut s = setup();
        let file = s.music.join("Other/polar.mp3");
        let tracks = s.library.add_files(&[&file], &s.covers).unwrap();
        assert_eq!(tracks[0].folder_id, Some(s.folder));
        // The scan sees it as known.
        let summary = s.scan();
        assert_eq!(summary.added, 2);
        assert_eq!(s.titles().len(), 3);
    }

    #[test]
    fn a_single_track_keeps_its_history_when_its_folder_is_added() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("Later");
        fs::create_dir_all(&music).unwrap();
        let file = music.join("song.flac");
        fs::copy(fixtures().join("tagged.flac"), &file).unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();

        let single = library.add_files(&[&file], &covers).unwrap().remove(0);
        library.set_favorite(single.id, true).unwrap();
        library.record_play(single.id, 100).unwrap();

        let folder = library.add_folder(music.to_str().unwrap()).unwrap();
        let summary = library
            .scan_folder(folder.id, &covers, &AtomicBool::new(false), &mut |_| {})
            .unwrap();
        assert_eq!((summary.added, summary.unchanged), (0, 1));
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        assert_eq!(tracks.len(), 1, "no duplicate");
        assert_eq!(tracks[0].id, single.id);
        assert_eq!(tracks[0].folder_id, Some(folder.id));
        assert!(tracks[0].favorite);
        assert_eq!(tracks[0].play_count, 1);
    }

    #[test]
    fn retagging_a_single_track_refreshes_it() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("song.wav");
        fs::copy(fixtures().join("untagged.wav"), &file).unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let single = library.add_files(&[&file], &covers).unwrap().remove(0);
        assert_eq!(single.title, "song");

        let values = media::TagValues {
            title: "Named".into(),
            ..media::TagValues::default()
        };
        media::write_tags(&file, &values, &media::CoverChange::Keep).unwrap();
        library.refresh_files(&[&file], &covers).unwrap();
        let after = library.track(single.id).unwrap().unwrap();
        assert_eq!(after.title, "Named");
        assert_eq!(after.folder_id, None);
    }

    #[test]
    fn refreshing_a_file_picks_up_new_tags() {
        let mut s = setup();
        s.scan();
        let path = s.music.join("Other/Untitled Take.wav");
        let before = s
            .library
            .track_by_path(path.to_str().unwrap())
            .unwrap()
            .unwrap();
        s.library.set_favorite(before.id, true).unwrap();
        let values = media::TagValues {
            title: "Named".into(),
            artist: "Someone".into(),
            album: "Fresh Album".into(),
            ..media::TagValues::default()
        };
        media::write_tags(&path, &values, &media::CoverChange::Keep).unwrap();
        s.library.refresh_files(&[&path], &s.covers).unwrap();

        let after = s
            .library
            .track_by_path(path.to_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(after.id, before.id);
        assert_eq!(after.added_at, before.added_at);
        assert!(after.favorite);
        assert_eq!(after.title, "Named");
        assert_eq!(after.album.as_deref(), Some("Fresh Album"));
        assert_eq!(s.library.search("fresh", 10).unwrap().albums.len(), 1);
        // A rescan sees the rewritten file as unchanged: refresh stored its new size/mtime.
        assert_eq!(s.scan().unchanged, 3);

        // Files outside the library are ignored.
        let outside = s._tmp.path().join("elsewhere.wav");
        fs::copy(fixtures().join("untagged.wav"), &outside).unwrap();
        s.library.refresh_files(&[&outside], &s.covers).unwrap();
        assert!(
            s.library
                .track_by_path(outside.to_str().unwrap())
                .unwrap()
                .is_none()
        );
    }
}
