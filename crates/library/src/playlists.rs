use rusqlite::{OptionalExtension, Transaction, params};

use crate::model::Playlist;
use crate::{Error, Library, Result, now};

/// The cover is the one the user chose, else the first one among the playlist's tracks.
pub(crate) const PLAYLIST_SELECT: &str = "SELECT p.id, p.name, COUNT(t.id),
        COALESCE(SUM(t.duration), 0), p.created_at, p.updated_at, p.source_url,
        COALESCE(p.cover, (
            SELECT COALESCE(a.custom_cover, c.cover, a.cover) FROM playlist_items ci
                JOIN tracks c ON c.id = ci.track_id LEFT JOIN albums a ON a.id = c.album_id
                WHERE ci.playlist_id = p.id
                    AND COALESCE(a.custom_cover, c.cover, a.cover) IS NOT NULL
                ORDER BY ci.position LIMIT 1)),
        p.cover IS NOT NULL
    FROM playlists p
    LEFT JOIN playlist_items pi ON pi.playlist_id = p.id
    LEFT JOIN tracks t ON t.id = pi.track_id";

pub(crate) fn playlist_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Playlist> {
    Ok(Playlist {
        id: row.get(0)?,
        name: row.get(1)?,
        track_count: row.get(2)?,
        duration: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        source_url: row.get(6)?,
        cover: row.get(7)?,
        custom_cover: row.get(8)?,
    })
}

fn valid_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        Err(Error::Invalid("a playlist needs a name".into()))
    } else {
        Ok(name)
    }
}

/// Track ids of a playlist in order.
fn items(tx: &Transaction<'_>, playlist_id: i64) -> Result<Vec<i64>> {
    let exists: Option<i64> = tx
        .query_row(
            "SELECT id FROM playlists WHERE id = ?",
            [playlist_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(Error::Invalid(format!("no playlist {playlist_id}")));
    }
    let mut statement =
        tx.prepare("SELECT track_id FROM playlist_items WHERE playlist_id = ? ORDER BY position")?;
    let ids = statement
        .query_map([playlist_id], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids)
}

/// Rewrites a playlist with positions 0..n. Playlists are small enough (thousands of rows at
/// most) that renumbering everything is simpler than shifting ranges, and it heals the gaps
/// left when tracks are deleted.
pub(crate) fn write_items(tx: &Transaction<'_>, playlist_id: i64, ids: &[i64]) -> Result<()> {
    tx.execute(
        "DELETE FROM playlist_items WHERE playlist_id = ?",
        [playlist_id],
    )?;
    let mut insert = tx.prepare(
        "INSERT INTO playlist_items (playlist_id, position, track_id) VALUES (?1, ?2, ?3)",
    )?;
    for (position, id) in ids.iter().enumerate() {
        insert.execute(params![playlist_id, position as i64, id])?;
    }
    tx.execute(
        "UPDATE playlists SET updated_at = ?2 WHERE id = ?1",
        params![playlist_id, now()],
    )?;
    Ok(())
}

impl Library {
    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut statement = self.conn.prepare(&format!(
            "{PLAYLIST_SELECT} GROUP BY p.id ORDER BY p.name COLLATE NOCASE, p.id"
        ))?;
        let playlists = statement
            .query_map([], playlist_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(playlists)
    }

    pub fn playlist(&self, id: i64) -> Result<Option<Playlist>> {
        Ok(self
            .conn
            .query_row(
                &format!("{PLAYLIST_SELECT} WHERE p.id = ? GROUP BY p.id"),
                [id],
                playlist_from_row,
            )
            .optional()?)
    }

    pub fn create_playlist(&self, name: &str) -> Result<Playlist> {
        let name = valid_name(name)?;
        let at = now();
        self.conn.execute(
            "INSERT INTO playlists (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
            params![name, at],
        )?;
        self.playlist(self.conn.last_insert_rowid())?
            .ok_or_else(|| Error::Invalid("playlist vanished".into()))
    }

    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE playlists SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, valid_name(name)?, now()],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM playlists WHERE id = ?", [id])?;
        Ok(())
    }

    /// Inserts tracks at `index` (clamped), or appends them.
    pub fn add_to_playlist(
        &mut self,
        playlist_id: i64,
        track_ids: &[i64],
        index: Option<usize>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut ids = items(&tx, playlist_id)?;
        let at = index.unwrap_or(ids.len()).min(ids.len());
        ids.splice(at..at, track_ids.iter().copied());
        write_items(&tx, playlist_id, &ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Removes the items at the given indices; out-of-range indices are ignored.
    pub fn remove_from_playlist(&mut self, playlist_id: i64, indices: &[usize]) -> Result<()> {
        let tx = self.conn.transaction()?;
        let ids: Vec<i64> = items(&tx, playlist_id)?
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !indices.contains(index))
            .map(|(_, id)| id)
            .collect();
        write_items(&tx, playlist_id, &ids)?;
        tx.commit()?;
        Ok(())
    }

    /// Moves the item at `from` so it ends up at index `to`.
    pub fn move_in_playlist(&mut self, playlist_id: i64, from: usize, to: usize) -> Result<()> {
        let tx = self.conn.transaction()?;
        let mut ids = items(&tx, playlist_id)?;
        if from >= ids.len() {
            return Err(Error::Invalid(format!("no item {from}")));
        }
        let id = ids.remove(from);
        ids.insert(to.min(ids.len()), id);
        write_items(&tx, playlist_id, &ids)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackFilter;
    use crate::tracks::tests::sample_library;

    fn playlist_titles(library: &Library, id: i64) -> Vec<String> {
        library
            .tracks(&TrackFilter::Playlist(id))
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect()
    }

    fn ids_by_title(library: &Library) -> impl Fn(&str) -> i64 + use<> {
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        move |title: &str| tracks.iter().find(|t| t.title == title).unwrap().id
    }

    #[test]
    fn creates_lists_and_renames() {
        let library = sample_library();
        assert!(library.create_playlist("   ").is_err());
        let b = library.create_playlist(" Road trip ").unwrap();
        library.create_playlist("ambient").unwrap();
        assert_eq!(b.name, "Road trip");
        assert_eq!(b.track_count, 0);

        library.rename_playlist(b.id, "Zzz").unwrap();
        let names: Vec<_> = library
            .playlists()
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["ambient", "Zzz"]);
    }

    #[test]
    fn adds_inserts_moves_and_removes_items() {
        let mut library = sample_library();
        let id = ids_by_title(&library);
        let playlist = library.create_playlist("Mix").unwrap();
        library
            .add_to_playlist(playlist.id, &[id("First"), id("Second")], None)
            .unwrap();
        library
            .add_to_playlist(playlist.id, &[id("Aurora")], Some(1))
            .unwrap();
        // Duplicates are allowed, like in any music player.
        library
            .add_to_playlist(playlist.id, &[id("First")], Some(99))
            .unwrap();
        assert_eq!(
            playlist_titles(&library, playlist.id),
            ["First", "Aurora", "Second", "First"]
        );

        library.move_in_playlist(playlist.id, 0, 2).unwrap();
        assert_eq!(
            playlist_titles(&library, playlist.id),
            ["Aurora", "Second", "First", "First"]
        );
        library
            .remove_from_playlist(playlist.id, &[1, 3, 42])
            .unwrap();
        assert_eq!(playlist_titles(&library, playlist.id), ["Aurora", "First"]);

        let stored = library.playlist(playlist.id).unwrap().unwrap();
        assert_eq!(stored.track_count, 2);
        assert_eq!(stored.duration, 400.0);
    }

    #[test]
    fn deleted_tracks_leave_no_holes() {
        let mut library = sample_library();
        let id = ids_by_title(&library);
        let playlist = library.create_playlist("Mix").unwrap();
        library
            .add_to_playlist(
                playlist.id,
                &[id("First"), id("Aurora"), id("Second")],
                None,
            )
            .unwrap();
        let tx = library.conn.transaction().unwrap();
        crate::tracks::delete_tracks(&tx, &[id("Aurora")]).unwrap();
        tx.commit().unwrap();

        assert_eq!(playlist_titles(&library, playlist.id), ["First", "Second"]);
        // Index 1 is "Second" now, whatever positions the rows had.
        library.move_in_playlist(playlist.id, 1, 0).unwrap();
        assert_eq!(playlist_titles(&library, playlist.id), ["Second", "First"]);
    }

    #[test]
    fn deleting_a_playlist_keeps_the_tracks() {
        let mut library = sample_library();
        let id = ids_by_title(&library);
        let playlist = library.create_playlist("Mix").unwrap();
        library
            .add_to_playlist(playlist.id, &[id("First")], None)
            .unwrap();
        library.delete_playlist(playlist.id).unwrap();
        assert!(library.playlists().unwrap().is_empty());
        assert!(library.add_to_playlist(playlist.id, &[1], None).is_err());
        assert_eq!(library.tracks(&TrackFilter::All).unwrap().len(), 4);
    }
}
