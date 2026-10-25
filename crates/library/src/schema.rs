//! Database schema, upgraded in place with numbered migrations (`PRAGMA user_version`).

use rusqlite::Connection;

use crate::Result;

/// Each entry upgrades the schema by one version. Never edit a released migration; add a new one.
pub(crate) const MIGRATIONS: &[&str] = &[
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
    // 2: links (stage 4). `source_url` is the web page a track or playlist came from: a
    // streamed track's path is that URL; a downloaded one keeps it next to its file path.
    // `tracks.cover` is a track's own cover (a link's thumbnail), shown before the album's.
    r"
    ALTER TABLE tracks ADD COLUMN source_url TEXT;
    ALTER TABLE tracks ADD COLUMN cover TEXT;
    CREATE INDEX tracks_source ON tracks(source_url);
    ALTER TABLE playlists ADD COLUMN source_url TEXT;
    ",
    // 3: lyrics (stage 5). `lyrics_offset` is the user's sync adjustment for a track, whatever
    // the lyrics' source; `lyrics_lookups` remembers online searches that found nothing.
    // Tracks are re-read once (mtime 0) so lyrics in their tags get indexed for search, and
    // the index folds "ё" to "е" (see `search::fold`).
    r"
    ALTER TABLE tracks ADD COLUMN lyrics_offset INTEGER NOT NULL DEFAULT 0;
    CREATE TABLE lyrics_lookups (
        track_id    INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        checked_at  INTEGER NOT NULL
    );
    UPDATE tracks SET file_mtime = 0 WHERE folder_id IS NOT NULL;
    UPDATE track_search SET
        title = replace(replace(title, 'ё', 'е'), 'Ё', 'Е'),
        artist = replace(replace(artist, 'ё', 'е'), 'Ё', 'Е'),
        album = replace(replace(album, 'ё', 'е'), 'Ё', 'Е'),
        album_artist = replace(replace(album_artist, 'ё', 'е'), 'Ё', 'Е'),
        lyrics = replace(replace(lyrics, 'ё', 'е'), 'Ё', 'Е');
    ",
    // 4: several artists per track. `track_artists` lists them in order (`tracks.artist_id`
    // stays the first, the main one); `artist_credit` keeps the credit they were split from
    // ("A feat. B"), so changing the splitting setting can split it again. Existing credits
    // are split once on the next open (`artists.resplitPending`), and files are re-read for
    // tags with several artist values. `artist_search` finds artists by name.
    r"
    CREATE TABLE track_artists (
        track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        artist_id  INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
        position   INTEGER NOT NULL,
        PRIMARY KEY (track_id, position)
    );
    CREATE INDEX track_artists_artist ON track_artists(artist_id, track_id);
    CREATE INDEX albums_artist ON albums(artist_id);
    ALTER TABLE tracks ADD COLUMN artist_credit TEXT;
    INSERT INTO track_artists (track_id, artist_id, position)
        SELECT id, artist_id, 0 FROM tracks WHERE artist_id IS NOT NULL;
    UPDATE tracks SET artist_credit = (SELECT name FROM artists WHERE id = tracks.artist_id);
    CREATE VIRTUAL TABLE artist_search USING fts5(
        name,
        tokenize = 'unicode61 remove_diacritics 2',
        prefix = '1 2 3'
    );
    INSERT INTO artist_search (rowid, name)
        SELECT id, replace(replace(name, 'ё', 'е'), 'Ё', 'Е') FROM artists;
    UPDATE tracks SET file_mtime = 0 WHERE folder_id IS NOT NULL;
    INSERT OR REPLACE INTO session (key, value) VALUES ('artists.resplitPending', 'true');
    ",
    // 5: covers the user chose. They are shown before automatic ones and nothing automatic
    // writes them: scans and links set `albums.cover`, never `albums.custom_cover`. Artists
    // and playlists have no stored automatic cover (theirs comes from their tracks).
    r"
    ALTER TABLE albums ADD COLUMN custom_cover TEXT;
    ALTER TABLE artists ADD COLUMN cover TEXT;
    ALTER TABLE playlists ADD COLUMN cover TEXT;
    ",
    // 6: details of a link the user corrected (title, artists, album): yt-dlp's later
    // descriptions of it (opening the link again, filling in its playlist) leave them alone.
    r"
    ALTER TABLE tracks ADD COLUMN edited INTEGER NOT NULL DEFAULT 0;
    ",
    // 7: the page an album came from (an album link), so its library page can read the
    // listing again and number tracks stored before a listing placed them.
    r"
    ALTER TABLE albums ADD COLUMN source_url TEXT;
    ",
    // 8: loudness Vitra measured for volume normalization (EBU R128), by path, so files
    // outside the library and links have it too; size and mtime tell when it is stale.
    r"
    CREATE TABLE loudness (
        path        TEXT PRIMARY KEY,
        integrated  REAL NOT NULL,
        peak        REAL,
        file_size   INTEGER,
        file_mtime  INTEGER
    );
    ",
    // 9: playlists imported from other services and files (stage 9). Their tracks may not
    // be in the library yet: each place in a playlist is a track, an entry still being
    // looked for, or both (an entry matched to a track, kept for its alternatives and for
    // importing the list again). An entry keeps what the source said and how the search
    // went: `state` pending | sure | doubtful | missing | picked, `candidates` the
    // alternatives found (JSON). `playlist_items` is rebuilt with `track_id` optional.
    r"
    CREATE TABLE import_entries (
        id           INTEGER PRIMARY KEY,
        playlist_id  INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        title        TEXT NOT NULL,
        artists      TEXT NOT NULL DEFAULT '[]',
        album        TEXT,
        duration     REAL,
        isrc         TEXT,
        url          TEXT,
        path         TEXT,
        swappable    INTEGER NOT NULL DEFAULT 0,
        state        TEXT NOT NULL DEFAULT 'pending',
        candidates   TEXT NOT NULL DEFAULT '[]',
        checked_at   INTEGER
    );
    CREATE INDEX import_entries_playlist ON import_entries(playlist_id);
    CREATE INDEX import_entries_state ON import_entries(state);

    CREATE TABLE playlist_items_new (
        playlist_id  INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
        position     INTEGER NOT NULL,
        track_id     INTEGER REFERENCES tracks(id) ON DELETE CASCADE,
        entry_id     INTEGER REFERENCES import_entries(id) ON DELETE CASCADE,
        PRIMARY KEY (playlist_id, position),
        CHECK (track_id IS NOT NULL OR entry_id IS NOT NULL)
    );
    INSERT INTO playlist_items_new (playlist_id, position, track_id)
        SELECT playlist_id, position, track_id FROM playlist_items;
    DROP TABLE playlist_items;
    ALTER TABLE playlist_items_new RENAME TO playlist_items;
    CREATE INDEX playlist_items_track ON playlist_items(track_id);
    CREATE INDEX playlist_items_entry ON playlist_items(entry_id);
    ",
    // 10: My Wave (stage 10). What each track sounds like (`mood::Features`), measured once
    // per analysis `version` from the file (`source` file; its size and mtime tell when it
    // changed) or a stretch of a link's stream (`source` stream). A failure is a row with
    // `error` set, so a file that can't be decoded isn't tried again on every launch.
    // `wave_marks` keeps skips (`in_wave` or not) and dislikes, which the wave weighs.
    r"
    CREATE TABLE track_features (
        track_id          INTEGER PRIMARY KEY REFERENCES tracks(id) ON DELETE CASCADE,
        version           INTEGER NOT NULL,
        source            TEXT NOT NULL,
        file_size         INTEGER,
        file_mtime        INTEGER,
        energy            REAL,
        spread            REAL,
        tempo             REAL,
        tempo_confidence  REAL,
        brightness        REAL,
        valence           REAL,
        error             TEXT,
        analyzed_at       INTEGER NOT NULL
    );
    CREATE TABLE wave_marks (
        id         INTEGER PRIMARY KEY,
        track_id   INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
        kind       TEXT NOT NULL,
        in_wave    INTEGER NOT NULL DEFAULT 0,
        marked_at  INTEGER NOT NULL
    );
    CREATE INDEX wave_marks_track ON wave_marks(track_id);
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
