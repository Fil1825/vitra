use std::collections::HashMap;
use std::path::Path;

use rusqlite::{OptionalExtension, params};

use crate::model::Folder;
use crate::tracks::{delete_tracks, prune};
use crate::{Error, Library, Result, now};

/// A file the library already knows: what an incremental scan compares against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KnownFile {
    pub id: i64,
    pub size: i64,
    pub mtime: i64,
    /// Lets a moved file be recognized by size and title.
    pub title: String,
}

const FOLDER_SELECT: &str = "SELECT f.id, f.path, f.added_at,
        (SELECT COUNT(*) FROM tracks WHERE folder_id = f.id)
    FROM folders f";

fn folder_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Folder> {
    Ok(Folder {
        id: row.get(0)?,
        path: row.get(1)?,
        added_at: row.get(2)?,
        track_count: row.get(3)?,
    })
}

/// `path` without trailing separators (but `/` stays `/`).
fn normalize(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        path
    } else {
        trimmed
    }
}

impl Library {
    pub fn folders(&self) -> Result<Vec<Folder>> {
        let mut statement = self
            .conn
            .prepare(&format!("{FOLDER_SELECT} ORDER BY f.path"))?;
        let folders = statement
            .query_map([], folder_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(folders)
    }

    pub fn folder(&self, id: i64) -> Result<Option<Folder>> {
        Ok(self
            .conn
            .query_row(
                &format!("{FOLDER_SELECT} WHERE f.id = ?"),
                [id],
                folder_from_row,
            )
            .optional()?)
    }

    /// Adds a folder to the library. Adding a folder that is already there returns it; a
    /// folder inside one already in the library is refused; library folders inside the new
    /// one are merged into it (their tracks keep their ids, play counts and playlists).
    pub fn add_folder(&mut self, path: &str) -> Result<Folder> {
        let path = normalize(path);
        if !Path::new(path).is_absolute() {
            return Err(Error::Invalid(format!("not an absolute path: {path}")));
        }
        let tx = self.conn.transaction()?;
        let existing: Vec<(i64, String)> = {
            let mut statement = tx.prepare("SELECT id, path FROM folders")?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };

        let new = Path::new(path);
        if let Some((id, _)) = existing.iter().find(|(_, p)| Path::new(p) == new) {
            let id = *id;
            drop(tx);
            return self
                .folder(id)?
                .ok_or_else(|| Error::Invalid("folder vanished".into()));
        }
        if let Some((_, parent)) = existing.iter().find(|(_, p)| new.starts_with(p)) {
            return Err(Error::Invalid(format!(
                "{path} is already in the library as part of {parent}"
            )));
        }

        tx.execute(
            "INSERT INTO folders (path, added_at) VALUES (?1, ?2)",
            params![path, now()],
        )?;
        let id = tx.last_insert_rowid();
        for (child, child_path) in &existing {
            if Path::new(child_path).starts_with(new) {
                tx.execute(
                    "UPDATE tracks SET folder_id = ?1 WHERE folder_id = ?2",
                    params![id, child],
                )?;
                tx.execute("DELETE FROM folders WHERE id = ?", [child])?;
            }
        }
        // Single files added earlier that live in this folder join it: the scan then sees them
        // as known instead of adding duplicates, and they keep favorites and history.
        let single: Vec<(i64, String)> = {
            let mut statement =
                tx.prepare("SELECT id, path FROM tracks WHERE folder_id IS NULL")?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        for (track, track_path) in single {
            if Path::new(&track_path).starts_with(new) {
                tx.execute(
                    "UPDATE tracks SET folder_id = ?1 WHERE id = ?2",
                    params![id, track],
                )?;
            }
        }
        tx.commit()?;
        self.folder(id)?
            .ok_or_else(|| Error::Invalid("folder vanished".into()))
    }

    /// Removes a folder and its tracks from the library (the files stay on disk).
    pub fn remove_folder(&mut self, id: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let ids: Vec<i64> = {
            let mut statement = tx.prepare("SELECT id FROM tracks WHERE folder_id = ?")?;
            statement
                .query_map([id], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        delete_tracks(&tx, &ids)?;
        tx.execute("DELETE FROM folders WHERE id = ?", [id])?;
        prune(&tx)?;
        tx.commit()?;
        Ok(())
    }

    /// Removes single tracks (added on their own, outside any folder) from the library; the
    /// files stay on disk. Tracks inside library folders are left alone, since the next scan
    /// would bring them back anyway. Returns how many were removed.
    pub fn remove_single_tracks(&mut self, ids: &[i64]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut single = Vec::new();
        for &id in ids {
            let found: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE id = ? AND folder_id IS NULL",
                    [id],
                    |row| row.get(0),
                )
                .optional()?;
            single.extend(found);
        }
        delete_tracks(&tx, &single)?;
        prune(&tx)?;
        tx.commit()?;
        Ok(single.len())
    }

    /// The folder (if any) that contains `path`.
    pub fn folder_containing(&self, path: &Path) -> Result<Option<Folder>> {
        Ok(self
            .folders()?
            .into_iter()
            .find(|folder| path.starts_with(&folder.path)))
    }

    /// Files of a folder by path.
    pub(crate) fn known_files(&self, folder_id: i64) -> Result<HashMap<String, KnownFile>> {
        let mut statement = self.conn.prepare(
            "SELECT path, id, file_size, file_mtime, title FROM tracks WHERE folder_id = ?",
        )?;
        let files = statement
            .query_map([folder_id], |row| {
                Ok((
                    row.get(0)?,
                    KnownFile {
                        id: row.get(1)?,
                        size: row.get(2)?,
                        mtime: row.get(3)?,
                        title: row.get(4)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackFilter;
    use crate::tracks::tests::scanned;

    #[test]
    fn adding_is_idempotent_and_ignores_trailing_slashes() {
        let mut library = Library::open_in_memory().unwrap();
        let a = library.add_folder("/music/").unwrap();
        let b = library.add_folder("/music").unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.path, "/music");
        assert_eq!(library.folders().unwrap().len(), 1);
    }

    #[test]
    fn refuses_relative_paths_and_nested_folders() {
        let mut library = Library::open_in_memory().unwrap();
        library.add_folder("/music").unwrap();
        assert!(library.add_folder("relative/dir").is_err());
        assert!(library.add_folder("/music/rock").is_err());
        // A sibling that only shares a prefix is fine.
        assert!(library.add_folder("/musical").is_ok());
    }

    #[test]
    fn a_parent_folder_absorbs_its_children() {
        let mut library = Library::open_in_memory().unwrap();
        let child = library.add_folder("/music/rock").unwrap();
        library
            .upsert_tracks(
                Some(child.id),
                &[scanned("/music/rock/a.flac", "A", "X", "Y", 1)],
            )
            .unwrap();
        let track = library
            .track_by_path("/music/rock/a.flac")
            .unwrap()
            .unwrap();

        let parent = library.add_folder("/music").unwrap();
        let folders = library.folders().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].id, parent.id);
        assert_eq!(folders[0].track_count, 1);
        let known = library.known_files(parent.id).unwrap();
        assert_eq!(known["/music/rock/a.flac"].id, track.id);
    }

    #[test]
    fn removing_a_folder_removes_its_tracks() {
        let mut library = Library::open_in_memory().unwrap();
        let a = library.add_folder("/a").unwrap();
        let b = library.add_folder("/b").unwrap();
        library
            .upsert_tracks(Some(a.id), &[scanned("/a/1.mp3", "One", "X", "Album A", 1)])
            .unwrap();
        library
            .upsert_tracks(Some(b.id), &[scanned("/b/1.mp3", "Two", "Y", "Album B", 1)])
            .unwrap();
        library.remove_folder(a.id).unwrap();

        let titles: Vec<_> = library
            .tracks(&TrackFilter::All)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["Two"]);
        assert_eq!(library.artists().unwrap().len(), 1);
        assert!(library.search("one", 10).unwrap().tracks.is_empty());
    }

    #[test]
    fn a_new_folder_takes_in_single_tracks_inside_it() {
        let mut library = Library::open_in_memory().unwrap();
        library
            .upsert_tracks(
                None,
                &[
                    scanned("/music/a/1.mp3", "Inside", "X", "Y", 1),
                    scanned("/elsewhere/2.mp3", "Outside", "X", "Y", 2),
                ],
            )
            .unwrap();
        let inside = library.track_by_path("/music/a/1.mp3").unwrap().unwrap();
        library.set_favorite(inside.id, true).unwrap();

        let folder = library.add_folder("/music").unwrap();
        assert_eq!(folder.track_count, 1);
        let known = library.known_files(folder.id).unwrap();
        assert_eq!(known["/music/a/1.mp3"].id, inside.id);
        let outside = library.track_by_path("/elsewhere/2.mp3").unwrap().unwrap();
        assert_eq!(outside.folder_id, None);
        assert!(library.track(inside.id).unwrap().unwrap().favorite);
    }

    #[test]
    fn only_single_tracks_can_be_removed() {
        let mut library = Library::open_in_memory().unwrap();
        let folder = library.add_folder("/music").unwrap();
        library
            .upsert_tracks(
                Some(folder.id),
                &[scanned("/music/1.mp3", "Kept", "X", "A", 1)],
            )
            .unwrap();
        library
            .upsert_tracks(None, &[scanned("/tmp/2.mp3", "Single", "Y", "B", 1)])
            .unwrap();
        let ids: Vec<i64> = library
            .tracks(&TrackFilter::All)
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();

        assert_eq!(library.remove_single_tracks(&ids).unwrap(), 1);
        let left: Vec<_> = library
            .tracks(&TrackFilter::All)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(left, ["Kept"]);
        // Its album and artist went with it.
        assert_eq!(library.artists().unwrap().len(), 1);
        assert!(library.search("single", 10).unwrap().tracks.is_empty());
    }

    #[test]
    fn finds_the_containing_folder() {
        let mut library = Library::open_in_memory().unwrap();
        let folder = library.add_folder("/music").unwrap();
        let found = library
            .folder_containing(Path::new("/music/a/b.flac"))
            .unwrap();
        assert_eq!(found.map(|f| f.id), Some(folder.id));
        assert!(
            library
                .folder_containing(Path::new("/musical/b.flac"))
                .unwrap()
                .is_none()
        );
    }
}
