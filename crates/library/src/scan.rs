//! Folder scanning: lists audio files, reads tags of new and changed ones in parallel,
//! drops files that are gone, follows files that were moved or renamed, and caches album
//! covers as small JPEGs.
//!
//! Scans are incremental: a file whose size and modification time match the database is not
//! read again, so rescanning a large, unchanged library only costs a directory walk.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant, UNIX_EPOCH};

use media::TrackMetadata;
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::folders::KnownFile;
use crate::model::ScannedTrack;
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

fn mtime(metadata: &fs::Metadata) -> i64 {
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
/// tells, since mpv still plays them.
fn read_metadata(path: &Path) -> TrackMetadata {
    match media::read_track(path) {
        Ok(info) => info.metadata,
        Err(err) => {
            log::debug!("no tags for {}: {err}", path.display());
            TrackMetadata {
                format: path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(str::to_ascii_uppercase),
                ..TrackMetadata::default()
            }
        }
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
            media::read_track(Path::new(path))
                .ok()
                .and_then(|info| info.embedded_artwork)
        })
        .or_else(|| {
            paths
                .first()
                .and_then(|path| media::folder_artwork(Path::new(path)))
        })
        .map(|artwork| artwork.bytes)
}

/// Writes the cover thumbnail if it isn't cached yet; returns its id.
fn cache_cover(bytes: &[u8], covers_dir: &Path) -> Option<String> {
    let id = cover_id(bytes);
    let target = covers_dir.join(format!("{id}.jpg"));
    if target.is_file() {
        return Some(id);
    }
    let thumbnail = media::cover_thumbnail(bytes, COVER_SIZE)
        .inspect_err(|err| log::info!("cannot decode a cover: {err}"))
        .ok()?;
    // Written under a temporary name first, so a half-written file is never served.
    let partial = covers_dir.join(format!("{id}.jpg.part"));
    let written = fs::create_dir_all(covers_dir)
        .and_then(|()| fs::write(&partial, thumbnail))
        .and_then(|()| fs::rename(&partial, &target));
    match written {
        Ok(()) => Some(id),
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
            let id = upsert_track(&tx, Some(folder_id), &track, added_at)?;
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
        let mut touched_albums = HashSet::new();
        let mut tracks = Vec::new();
        for path in paths {
            let Some(key) = path.to_str() else {
                continue;
            };
            let existing: Option<(Option<i64>, Option<i64>)> = self
                .conn
                .query_row(
                    "SELECT folder_id, album_id FROM tracks WHERE path = ?",
                    [key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let folder_id = match existing {
                Some((folder_id, album_id)) => {
                    // The old album may lose its cover source.
                    touched_albums.extend(album_id);
                    folder_id
                }
                None => self.folder_containing(path)?.map(|folder| folder.id),
            };
            let Some(folder_id) = folder_id else {
                continue;
            };
            let Ok(metadata) = fs::metadata(path) else {
                continue;
            };
            tracks.push((
                folder_id,
                ScannedTrack {
                    path: key.to_owned(),
                    size: metadata.len(),
                    mtime: mtime(&metadata),
                    metadata: read_metadata(path),
                },
            ));
        }

        let tx = self.conn.transaction()?;
        let at = now();
        for (folder_id, track) in &tracks {
            let id = upsert_track(&tx, Some(*folder_id), track, at)?;
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
        let mut ignore = |_| {};
        let mut reporter = Reporter {
            folder_id: 0,
            callback: &mut ignore,
            last: None,
        };
        self.update_covers(&albums, covers_dir, &AtomicBool::new(false), &mut reporter)?;
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
            let id = name.strip_suffix(".jpg").or(name.strip_suffix(".jpg.part"));
            if id.is_some_and(|id| !used.contains(id)) && fs::remove_file(&path).is_ok() {
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
