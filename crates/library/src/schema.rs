//! Database schema, upgraded in place with numbered migrations (`PRAGMA user_version`).

use rusqlite::Connection;

use crate::Result;

/// Each entry upgrades the schema by one version. Never edit a released migration; add a new one.
const MIGRATIONS: &[&str] = &[
    // 1: initial schema.
    r"
    CREATE TABLE folders (
        id        INTEGER PRIMARY KEY,
        path      TEXT NOT NULL UNIQUE,
        added_at  INTEGER NOT NULL
    );

    CREATE TABLE artists (
        id    INTEGER PRIMARY KEY,
        name  TEXT NOT NULL UNIQUE COLLATE NOCASE
    );

    -- An album is identified by its title and album artist (or track artist).
    CREATE TABLE albums (
        id         INTEGER PRIMARY KEY,
        title      TEXT NOT NULL COLLATE NOCASE,
        artist_id  INTEGER REFERENCES artists(id) ON DELETE SET NULL,
        year       INTEGER,
        -- Id of the cached cover thumbnail (covers/<id>.jpg), if any.
        cover      TEXT,
        UNIQUE (title, artist_id)
    );

    CREATE TABLE tracks (
        id              INTEGER PRIMARY KEY,
        path            TEXT NOT NULL UNIQUE,
        folder_id       INTEGER REFERENCES folders(id) ON DELETE CASCADE,
        title           TEXT NOT NULL,
        artist_id       INTEGER REFERENCES artists(id) ON DELETE SET NULL,
        album_id        INTEGER REFERENCES albums(id) ON DELETE SET NULL,
        album_artist    TEXT,
        genre           TEXT,
        year            INTEGER,
        track_number    INTEGER,
        disc_number     INTEGER,
        duration        REAL,
        format          TEXT,
        sample_rate     INTEGER,
        bit_depth       INTEGER,
        bitrate         INTEGER,
        -- Used by incremental scans: unchanged size and mtime means unchanged tags.
        file_size       INTEGER NOT NULL DEFAULT 0,
        file_mtime      INTEGER NOT NULL DEFAULT 0,
        added_at        INTEGER NOT NULL,
        play_count      INTEGER NOT NULL DEFAULT 0,
        last_played_at  INTEGER,
        favorite        INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX tracks_album ON tracks(album_id);
    CREATE INDEX tracks_artist ON tracks(artist_id);
    CREATE INDEX tracks_folder ON tracks(folder_id);
    CREATE INDEX tracks_added ON tracks(added_at);

    CREATE TABLE playlists (
        id          INTEGER PRIMARY KEY,
        name        TEXT NOT NULL,
        created_at  INTEGER NOT NULL,
        updated_at  INTEGER NOT NULL
    );

    CREATE TABLE playlist_items (
        playlist_id  INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        position     INTEGER NOT NULL,
        track_id     INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        PRIMARY KEY (playlist_id, position)
    );

    -- Lyrics saved in the app (stage 5); also indexed for search.
    CREATE TABLE lyrics (
        track_id    INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        text        TEXT NOT NULL,
        synced      INTEGER NOT NULL DEFAULT 0,
        source      TEXT,
        updated_at  INTEGER NOT NULL
    );

    CREATE TABLE history (
        id         INTEGER PRIMARY KEY,
        track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        played_at  INTEGER NOT NULL
    );
    CREATE INDEX history_played ON history(played_at);

    -- Small JSON blobs: the queue, playback position, volume, ...
    CREATE TABLE session (
        key    TEXT PRIMARY KEY,
        value  TEXT NOT NULL
    );

    -- Full-text index over tracks; rowid = tracks.id. `remove_diacritics 2` makes
    -- 'e' match 'é' and Russian 'е' match 'ё'.
    CREATE VIRTUAL TABLE track_search USING fts5(
        title, artist, album, album_artist, lyrics,
        tokenize = 'unicode61 remove_diacritics 2',
        prefix = '1 2 3'
    );
    ",
];

/// Brings the database up to the latest schema.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current.max(0) as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
        tx.commit()?;
        log::info!("library database migrated to version {}", index + 1);
    }
    Ok(())
}

#[cfg(test)]
pub fn version() -> i64 {
    MIGRATIONS.len() as i64
}
