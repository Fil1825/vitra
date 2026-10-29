//! Copies of files taken before the lyrics editor rewrites them, in `<app data>/backups/`.
//! Only the newest few are kept: they are a safety net for the last edits, not an archive.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP: usize = 10;

/// Copies `file` into `dir` as `<milliseconds>-<name>`, then removes the oldest copies beyond
/// [`KEEP`]. Returns the copy's path.
pub fn back_up(file: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    back_up_at(file, dir, millis)
}

fn back_up_at(file: &Path, dir: &Path, millis: u128) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let next = |n: u32| {
        n.checked_add(1)
            .ok_or_else(|| io::Error::other("too many backups at one timestamp"))
    };
    // Pruning frees low suffixes. Reusing those names would make a new copy look oldest
    // and delete it immediately when several edits happen within one millisecond.
    let mut n = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| stamp(&entry.file_name().to_string_lossy()))
        .filter(|(time, _)| *time == millis)
        .map(|(_, suffix)| suffix)
        .max()
        .map_or(Ok(0), next)?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let mut source = File::open(file)?;
    loop {
        let copy = dir.join(if n == 0 {
            format!("{millis}-{name}")
        } else {
            format!("{millis}.{n}-{name}")
        });
        // Reserve atomically: another writer must never have its backup overwritten.
        let mut destination = match OpenOptions::new().write(true).create_new(true).open(&copy) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                n = next(n)?;
                continue;
            }
            Err(err) => return Err(err),
        };
        let result = io::copy(&mut source, &mut destination)
            .and_then(|_| destination.set_permissions(source.metadata()?.permissions()));
        drop(destination);
        if let Err(err) = result {
            let _ = fs::remove_file(&copy);
            return Err(err);
        }
        prune(dir);
        return Ok(copy);
    }
}

/// The copy's time, from its name.
fn stamp(name: &str) -> Option<(u128, u32)> {
    let (time, _) = name.split_once('-')?;
    let (millis, n) = time.split_once('.').unwrap_or((time, "0"));
    Some((millis.parse().ok()?, n.parse().ok()?))
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut copies: Vec<((u128, u32), PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| Some((stamp(&entry.file_name().to_string_lossy())?, entry.path())))
        .collect();
    if copies.len() <= KEEP {
        return;
    }
    copies.sort();
    for (_, path) in &copies[..copies.len() - KEEP] {
        if let Err(err) = fs::remove_file(path) {
            log::info!("cannot remove the old backup {}: {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("Song - Name.flac");
        let dir = tmp.path().join("backups");
        for n in 0..KEEP + 3 {
            fs::write(&file, n.to_string()).unwrap();
            let copy = back_up(&file, &dir).unwrap();
            assert!(copy.to_string_lossy().ends_with("-Song - Name.flac"));
        }
        let mut kept: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        kept.sort_by_key(|text| text.parse::<usize>().unwrap());
        let expected: Vec<String> = (3..KEEP + 3).map(|n| n.to_string()).collect();
        assert_eq!(kept, expected);
    }

    #[test]
    fn pruning_never_reuses_a_same_millisecond_sequence() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("Song.flac");
        let dir = tmp.path().join("backups");
        for n in 0..KEEP + 3 {
            fs::write(&file, n.to_string()).unwrap();
            let copy = back_up_at(&file, &dir, 1234).unwrap();
            assert_eq!(fs::read_to_string(copy).unwrap(), n.to_string());
        }
        let mut kept: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| {
                fs::read_to_string(entry.unwrap().path())
                    .unwrap()
                    .parse::<usize>()
                    .unwrap()
            })
            .collect();
        kept.sort_unstable();
        assert_eq!(kept, (3..KEEP + 3).collect::<Vec<_>>());
    }
}
