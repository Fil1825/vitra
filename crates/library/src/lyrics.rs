//! Lyrics kept in the library: text saved in the app (by the user, or found online), the
//! user's sync offset per track, and which tracks were looked up online without a result.

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::tracks::searchable_lyrics;
use crate::{Library, Result, now};

/// Where saved lyrics came from.
pub const SOURCE_USER: &str = "user";
pub const SOURCE_ONLINE: &str = "lrclib";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLyrics {
    /// LRC or plain text.
    pub text: String,
    pub synced: bool,
    /// [`SOURCE_USER`] or [`SOURCE_ONLINE`].
    pub source: String,
    pub updated_at: i64,
}

impl Library {
    pub fn saved_lyrics(&self, track_id: i64) -> Result<Option<SavedLyrics>> {
        Ok(self
            .conn
            .query_row(
                "SELECT text, synced, COALESCE(source, ?2), updated_at FROM lyrics
                 WHERE track_id = ?1",
                params![track_id, SOURCE_USER],
                |row| {
                    Ok(SavedLyrics {
                        text: row.get(0)?,
                        synced: row.get(1)?,
                        source: row.get(2)?,
                        updated_at: row.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    /// The text of saved lyrics (what a download writes into its file).
    pub fn lyrics_text(&self, track_id: i64) -> Result<Option<String>> {
        Ok(self.saved_lyrics(track_id)?.map(|saved| saved.text))
    }

    /// Saves lyrics for a track (replacing any) and indexes their words for search.
    pub fn save_lyrics(&self, track_id: i64, text: &str, source: &str) -> Result<()> {
        let synced = lyrics::parse(text).synced;
        self.conn.execute(
            "INSERT INTO lyrics (track_id, text, synced, source, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (track_id) DO UPDATE SET text = excluded.text,
                 synced = excluded.synced, source = excluded.source,
                 updated_at = excluded.updated_at",
            params![track_id, text, synced, source, now()],
        )?;
        self.index_lyrics(track_id, Some(text))
    }

    /// Forgets saved lyrics; the index keeps whatever [`Self::index_lyrics`] is told next.
    pub fn delete_lyrics(&self, track_id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM lyrics WHERE track_id = ?", [track_id])?;
        Ok(())
    }

    /// Makes the lyrics shown for a track searchable (they may come from a `.lrc` file or a
    /// tag frame the scanner doesn't read).
    pub fn index_lyrics(&self, track_id: i64, text: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE track_search SET lyrics = ?2 WHERE rowid = ?1",
            params![track_id, text.map(searchable_lyrics)],
        )?;
        Ok(())
    }

    /// The user's sync adjustment for a track, in milliseconds (positive: lyrics later).
    pub fn lyrics_offset(&self, track_id: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT lyrics_offset FROM tracks WHERE id = ?",
                [track_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn set_lyrics_offset(&self, track_id: i64, offset_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET lyrics_offset = ?2 WHERE id = ?1",
            params![track_id, offset_ms],
        )?;
        Ok(())
    }

    /// When an online search last found nothing for a track.
    pub fn lyrics_lookup_at(&self, track_id: i64) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT checked_at FROM lyrics_lookups WHERE track_id = ?",
                [track_id],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn note_lyrics_lookup(&self, track_id: i64, at: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO lyrics_lookups (track_id, checked_at) VALUES (?1, ?2)
             ON CONFLICT (track_id) DO UPDATE SET checked_at = excluded.checked_at",
            params![track_id, at],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrackFilter;
    use crate::tracks::tests::sample_library;

    fn first_id(library: &Library) -> i64 {
        library.tracks(&TrackFilter::All).unwrap()[0].id
    }

    #[test]
    fn saves_replaces_and_deletes_lyrics() {
        let library = sample_library();
        let id = first_id(&library);
        assert_eq!(library.saved_lyrics(id).unwrap(), None);

        library
            .save_lyrics(id, "[00:01.00]Звёздный свет\n[00:03.00]glow", SOURCE_ONLINE)
            .unwrap();
        let saved = library.saved_lyrics(id).unwrap().unwrap();
        assert!(saved.synced);
        assert_eq!(saved.source, SOURCE_ONLINE);
        // Found by its words, not its timestamps.
        assert_eq!(library.search_track_ids("звездный").unwrap(), [id]);
        assert!(library.search_track_ids("00").unwrap().is_empty());

        library.save_lyrics(id, "plain words", SOURCE_USER).unwrap();
        let saved = library.saved_lyrics(id).unwrap().unwrap();
        assert!(!saved.synced);
        assert_eq!(saved.source, SOURCE_USER);
        assert_eq!(
            library.lyrics_text(id).unwrap().as_deref(),
            Some("plain words")
        );
        assert!(library.search_track_ids("звездный").unwrap().is_empty());

        library.delete_lyrics(id).unwrap();
        assert_eq!(library.saved_lyrics(id).unwrap(), None);
        library.index_lyrics(id, None).unwrap();
        assert!(library.search_track_ids("plain").unwrap().is_empty());
    }

    #[test]
    fn keeps_offsets_and_lookups_per_track() {
        let library = sample_library();
        let id = first_id(&library);
        assert_eq!(library.lyrics_offset(id).unwrap(), 0);
        library.set_lyrics_offset(id, -350).unwrap();
        assert_eq!(library.lyrics_offset(id).unwrap(), -350);
        assert_eq!(library.lyrics_offset(9_999).unwrap(), 0);

        assert_eq!(library.lyrics_lookup_at(id).unwrap(), None);
        library.note_lyrics_lookup(id, 100).unwrap();
        library.note_lyrics_lookup(id, 200).unwrap();
        assert_eq!(library.lyrics_lookup_at(id).unwrap(), Some(200));
    }

    #[test]
    fn tag_lyrics_are_searchable_and_saved_lyrics_win() {
        let mut library = Library::open_in_memory().unwrap();
        let mut track = crate::tracks::tests::scanned("/m/a.flac", "Song", "Artist", "Album", 1);
        track.metadata.lyrics = Some("[00:01.00]northern lights".into());
        let id = library.upsert_tracks(None, &[track.clone()]).unwrap()[0];
        assert_eq!(library.search_track_ids("northern").unwrap(), [id]);

        library
            .save_lyrics(id, "southern cross", SOURCE_USER)
            .unwrap();
        // A rescan of the file keeps indexing what the app shows.
        library.upsert_tracks(None, &[track]).unwrap();
        assert_eq!(library.search_track_ids("southern").unwrap(), [id]);
        assert!(library.search_track_ids("northern").unwrap().is_empty());
    }
}
