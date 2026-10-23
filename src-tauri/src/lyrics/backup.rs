//! Copies of files taken before the lyrics editor rewrites them, in `<app data>/backups/`.
//! Only the newest few are kept: they are a safety net for the last edits, not an archive.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP: usize = 10;

/// Copies `file` into `dir` as `<milliseconds>-<name>`, then removes the oldest copies beyond
/// [`KEEP`]. Returns the copy's path.
pub fn back_up(file: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let mut copy = dir.join(format!("{millis}-{name}"));
    let mut n = 1;
    while copy.exists() {
        copy = dir.join(format!("{millis}.{n}-{name}"));
        n += 1;
    }
    fs::copy(file, &copy)?;
    prune(dir);
    Ok(copy)
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
}
