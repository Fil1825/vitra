//! Watches library folders and rescans one a few seconds after its files stop changing.
//!
//! A rescan is incremental (unchanged files are only stat'ed), so rescanning the whole folder
//! is simpler and more robust than replaying individual events: renames, moved directories
//! and editors that write through temporary files all come out right.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use super::LibraryService;

/// Quiet time before a rescan; copying an album produces a burst of events.
const SETTLE: Duration = Duration::from_secs(3);
/// A folder that never stops changing (a long copy) is still rescanned this often.
const MAX_DELAY: Duration = Duration::from_secs(30);

pub struct FolderWatcher {
    watcher: RecommendedWatcher,
}

/// Whether a changed path can affect the library: audio files and directories (which have
/// no extension, or are gone already). Hidden files include the tag editor's temporary files.
fn relevant(path: &Path) -> bool {
    let hidden = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'));
    !hidden && (path.extension().is_none() || library::is_audio_file(path) || path.is_dir())
}

impl FolderWatcher {
    pub fn start(library: LibraryService) -> notify::Result<Self> {
        let (sender, receiver) = mpsc::channel::<PathBuf>();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else {
                return;
            };
            // Reads (including the scanner's own) are not changes.
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            for path in event.paths {
                if relevant(&path) {
                    let _ = sender.send(path);
                }
            }
        })?;

        std::thread::Builder::new()
            .name("library-watch".into())
            .spawn(move || {
                while let Ok(first) = receiver.recv() {
                    let mut changed = vec![first];
                    let started = Instant::now();
                    loop {
                        match receiver.recv_timeout(SETTLE) {
                            Ok(path) => {
                                changed.push(path);
                                if started.elapsed() >= MAX_DELAY {
                                    break;
                                }
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                            Err(RecvTimeoutError::Disconnected) => return,
                        }
                    }
                    rescan(&library, &changed);
                }
            })
            .map_err(|err| notify::Error::generic(&err.to_string()))?;

        Ok(Self { watcher })
    }

    pub fn watch(&mut self, path: &Path) {
        if let Err(err) = self.watcher.watch(path, RecursiveMode::Recursive) {
            log::warn!("cannot watch {}: {err}", path.display());
        }
    }

    pub fn unwatch(&mut self, path: &Path) {
        let _ = self.watcher.unwatch(path);
    }
}

fn rescan(library: &LibraryService, changed: &[PathBuf]) {
    let folders = match library.with(|db| db.folders()) {
        Ok(folders) => folders,
        Err(err) => {
            log::warn!("cannot list library folders: {err}");
            return;
        }
    };
    for folder in folders {
        if changed.iter().any(|path| path.starts_with(&folder.path)) {
            log::info!("{} changed; rescanning", folder.path);
            library.request_scan(folder.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_audio_files_and_directories_matter() {
        assert!(relevant(Path::new("/m/a.flac")));
        assert!(relevant(Path::new("/m/Some Album")));
        assert!(!relevant(Path::new("/m/cover.jpg")));
        assert!(!relevant(Path::new("/m/.a.flac.vitra-tmp")));
        assert!(!relevant(Path::new("/m/.DS_Store")));
    }
}
