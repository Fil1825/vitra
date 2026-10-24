//! Vitra's music library: an SQLite database of tracks, albums, artists, playlists, play
//! history and session state, plus the folder scanner that fills it.
//!
//! All methods are synchronous; the app calls them from blocking tasks. SQLite runs in WAL
//! mode, so a scan writing through one connection doesn't block queries on another.

mod artists;
mod covers;
mod folders;
mod loudness;
mod lyrics;
mod model;
mod playlists;
mod remote;
mod schema;
mod search;
mod session;
mod tracks;

pub mod scan;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

pub use artists::{ArtistSplitting, BUILTIN_EXCEPTIONS, split_artists};
pub use covers::CoverTarget;
pub use loudness::{FileStamp, Loudness};
pub use lyrics::{SOURCE_ONLINE, SOURCE_USER, SavedLyrics};
pub use model::*;
pub use scan::{ScanPhase, ScanProgress, ScanSummary, cover_id, is_audio_file};
pub use tracks::AlbumSort;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// One connection to the library database.
pub struct Library {
    conn: Connection,
}

impl Library {
    /// Opens (creating if needed) the database at `path` and migrates it.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::setup(Connection::open(path)?)
    }

    /// A private in-memory database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::setup(Connection::open_in_memory()?)
    }

    fn setup(mut conn: Connection) -> Result<Self> {
        // WAL lets the scanner write while the UI reads; the busy timeout covers the rest.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        schema::migrate(&mut conn)?;
        let mut library = Self { conn };
        library.resplit_if_pending()?;
        Ok(library)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }
}

/// Unix seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_a_new_database() {
        let library = Library::open_in_memory().unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
    }

    #[test]
    fn reopening_keeps_data_and_version() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let library = Library::open(&path).unwrap();
            library.session_set("k", "v").unwrap();
        }
        let library = Library::open(&path).unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
        assert_eq!(library.session_get("k").unwrap().as_deref(), Some("v"));
    }

    #[test]
    fn upgrades_a_database_of_the_first_release() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(schema::MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
            conn.execute(
                "INSERT INTO tracks (path, title, added_at) VALUES ('/m/a.mp3', 'A', 1)",
                [],
            )
            .unwrap();
        }
        let library = Library::open(&path).unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
        let track = library.track_by_path("/m/a.mp3").unwrap().unwrap();
        assert_eq!((track.title.as_str(), track.source_url), ("A", None));
    }

    #[test]
    fn migration_4_splits_stored_credits_into_artists() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &schema::MIGRATIONS[..3] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 3).unwrap();
            // A link stored by stage 4 with yt-dlp's artists joined into one name, on an
            // album filed under that joined name.
            conn.execute_batch(
                "INSERT INTO artists (id, name) VALUES (1, 'VILLIAN, madk1d'), (2, 'madk1d');
                 INSERT INTO albums (id, title, artist_id, cover) VALUES (1, 'Night', 1, 'c0ffee');
                 INSERT INTO tracks (id, path, title, artist_id, album_id, added_at, source_url)
                     VALUES (1, 'https://y/1', 'Duo', 1, 1, 1, 'https://y/1'),
                            (2, 'https://y/2', 'Solo', 2, NULL, 1, 'https://y/2');
                 INSERT INTO track_search (rowid, title, artist) VALUES
                     (1, 'Duo', 'VILLIAN, madk1d'), (2, 'Solo', 'madk1d');",
            )
            .unwrap();
        }
        let library = Library::open(&path).unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
        let names: Vec<String> = library
            .artists()
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["madk1d", "VILLIAN"]);
        let duo = library.track(1).unwrap().unwrap();
        assert_eq!(duo.artist.as_deref(), Some("VILLIAN, madk1d"));
        let album = library.album(duo.album_id.unwrap()).unwrap().unwrap();
        assert_eq!(
            (
                album.title.as_str(),
                album.artist.as_deref(),
                album.cover.as_deref()
            ),
            ("Night", Some("VILLIAN"), Some("c0ffee"))
        );
        let madk1d = library.artists().unwrap().remove(0);
        assert_eq!(madk1d.track_count, 2);
        assert_eq!(library.search("villian", 10).unwrap().artists.len(), 1);
        drop(library);
        // Only once.
        let library = Library::open(&path).unwrap();
        assert_eq!(library.artists().unwrap().len(), 2);
    }

    #[test]
    fn migration_6_lets_links_stored_before_it_be_corrected() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &schema::MIGRATIONS[..5] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 5).unwrap();
            conn.execute_batch(
                "DELETE FROM session;
                 INSERT INTO tracks (id, path, title, added_at, source_url)
                     VALUES (1, 'https://y/1', 'Artist - Song (Official Video)', 1, 'https://y/1');",
            )
            .unwrap();
        }
        let mut library = Library::open(&path).unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
        let edit = LinkEdit {
            title: "Song".into(),
            artists: vec!["Artist".into()],
            ..LinkEdit::default()
        };
        assert_eq!(
            library
                .edit_link("https://y/1", &edit, None, tmp.path())
                .unwrap()
                .title,
            "Song"
        );
    }

    #[test]
    fn migration_5_adds_custom_covers_and_keeps_the_automatic_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &schema::MIGRATIONS[..4] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 4).unwrap();
            conn.execute_batch(
                "DELETE FROM session;
                 INSERT INTO artists (id, name) VALUES (1, 'Sqwore');
                 INSERT INTO albums (id, title, artist_id, cover) VALUES (1, 'Summer', 1, 'c0ffee');
                 INSERT INTO tracks (id, path, title, artist_id, album_id, added_at)
                     VALUES (1, '/m/1.flac', 'One', 1, 1, 1);
                 INSERT INTO track_artists (track_id, artist_id, position) VALUES (1, 1, 0);
                 INSERT INTO playlists (id, name, created_at, updated_at) VALUES (1, 'Mix', 1, 1);
                 INSERT INTO playlist_items (playlist_id, position, track_id) VALUES (1, 0, 1);",
            )
            .unwrap();
        }
        let library = Library::open(&path).unwrap();
        assert_eq!(library.schema_version().unwrap(), schema::version());
        let album = library.album(1).unwrap().unwrap();
        assert_eq!(album.cover.as_deref(), Some("c0ffee"));
        assert!(!album.custom_cover);
        let artist = library.artist(1).unwrap().unwrap();
        assert_eq!(
            (artist.cover.as_deref(), artist.custom_cover),
            (Some("c0ffee"), false)
        );
        let playlist = library.playlist(1).unwrap().unwrap();
        assert_eq!(
            (playlist.cover.as_deref(), playlist.custom_cover),
            (Some("c0ffee"), false)
        );
        assert_eq!(
            library.track(1).unwrap().unwrap().cover.as_deref(),
            Some("c0ffee")
        );
    }

    #[test]
    fn migration_3_folds_yo_in_the_existing_index() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library.db");
        {
            let conn = Connection::open(&path).unwrap();
            for sql in &schema::MIGRATIONS[..2] {
                conn.execute_batch(sql).unwrap();
            }
            conn.pragma_update(None, "user_version", 2).unwrap();
            conn.execute_batch(
                "INSERT INTO tracks (id, path, title, added_at) VALUES (1, '/m/a.mp3', 'Ёлка', 1);
                 INSERT INTO track_search (rowid, title) VALUES (1, 'Ёлка');",
            )
            .unwrap();
        }
        let library = Library::open(&path).unwrap();
        assert_eq!(library.search_track_ids("елка").unwrap(), [1]);
    }
}
