//! Vitra's music library: an SQLite database of tracks, albums, artists, playlists, play
//! history and session state, plus the folder scanner that fills it.
//!
//! All methods are synchronous; the app calls them from blocking tasks. SQLite runs in WAL
//! mode, so a scan writing through one connection doesn't block queries on another.

mod folders;
mod model;
mod playlists;
mod schema;
mod search;
mod session;
mod tracks;

pub mod scan;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

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
        Ok(Self { conn })
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
}
