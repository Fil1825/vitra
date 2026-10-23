use std::collections::{HashMap, HashSet};
use std::path::Path;

use rusqlite::{OptionalExtension, Row, Transaction, params, params_from_iter};

use crate::artists::{
    ArtistSplitting, album_artist_name, artist_id, artist_names, write_track_artists,
};
use crate::model::{Album, Artist, ArtistRef, HistoryEntry, ScannedTrack, Track, TrackFilter};
use crate::search::fold;
use crate::{Library, Result, now};

/// A track's cover, for track `$t` on album `$al`: the one the user chose for the album, then
/// the album's for a link the user put there, then the track's own (a link's thumbnail),
/// then the album's.
macro_rules! track_cover {
    ($t:literal, $al:literal) => {
        concat!(
            "COALESCE(",
            $al,
            ".custom_cover, CASE WHEN ",
            $t,
            ".edited THEN ",
            $al,
            ".cover END, ",
            $t,
            ".cover, ",
            $al,
            ".cover)"
        )
    };
}

/// Columns of [`Track`], in [`track_from_row`] order. Joins `artists ar` (the main artist,
/// for sorting) and `albums al`; the artists come as a JSON array of `[id, name]`.
pub(crate) const TRACK_COLUMNS: &str = concat!(
    "t.id, t.path, t.title,
    (SELECT json_group_array(json_array(x.id, x.name) ORDER BY ta.position)
        FROM track_artists ta JOIN artists x ON x.id = ta.artist_id WHERE ta.track_id = t.id),
    t.artist_id, al.title,
    t.album_id, t.album_artist, t.genre, t.year, t.track_number, t.disc_number, t.duration,
    t.format, t.sample_rate, t.bit_depth, t.bitrate, t.favorite, t.play_count,
    t.last_played_at, t.added_at, ",
    track_cover!("t", "al"),
    ", t.folder_id, t.source_url"
);

pub(crate) const TRACK_FROM: &str = "tracks t
    LEFT JOIN artists ar ON ar.id = t.artist_id
    LEFT JOIN albums al ON al.id = t.album_id";

/// Album order within an artist, then disc and track number.
const ALBUM_ORDER: &str = "al.year, al.title COLLATE NOCASE, t.disc_number, t.track_number,
    t.title COLLATE NOCASE";

/// `[[id, name], ...]` as [`TRACK_COLUMNS`] selects it.
fn artists_from_json(json: Option<String>) -> Vec<ArtistRef> {
    let pairs: Vec<(i64, String)> = json
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    pairs
        .into_iter()
        .map(|(id, name)| ArtistRef { id, name })
        .collect()
}

pub(crate) fn track_from_row(row: &Row<'_>) -> rusqlite::Result<Track> {
    let artists = artists_from_json(row.get(3)?);
    let artist = (!artists.is_empty()).then(|| {
        artists
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    });
    Ok(Track {
        id: row.get(0)?,
        path: row.get(1)?,
        title: row.get(2)?,
        artist,
        artist_id: row.get(4)?,
        artists,
        album: row.get(5)?,
        album_id: row.get(6)?,
        album_artist: row.get(7)?,
        genre: row.get(8)?,
        year: row.get(9)?,
        track_number: row.get(10)?,
        disc_number: row.get(11)?,
        duration: row.get(12)?,
        format: row.get(13)?,
        sample_rate: row.get(14)?,
        bit_depth: row.get(15)?,
        bitrate: row.get(16)?,
        favorite: row.get(17)?,
        play_count: row.get(18)?,
        last_played_at: row.get(19)?,
        added_at: row.get(20)?,
        cover: row.get(21)?,
        folder_id: row.get(22)?,
        source_url: row.get(23)?,
    })
}

/// The user's cover comes first; an album without a cover of its own (only links in it)
/// shows one of its tracks' covers.
pub(crate) const ALBUM_SELECT: &str = "SELECT al.id, al.title, ar.name, al.artist_id, al.year,
        COALESCE(al.custom_cover, al.cover, MAX(t.cover)), al.custom_cover IS NOT NULL,
        COUNT(t.id), COALESCE(SUM(t.path = t.source_url), 0), COALESCE(SUM(t.duration), 0),
        MAX(t.added_at)
    FROM albums al
    LEFT JOIN artists ar ON ar.id = al.artist_id
    JOIN tracks t ON t.album_id = al.id";

pub(crate) fn album_from_row(row: &Row<'_>) -> rusqlite::Result<Album> {
    Ok(Album {
        id: row.get(0)?,
        title: row.get(1)?,
        artist: row.get(2)?,
        artist_id: row.get(3)?,
        year: row.get(4)?,
        cover: row.get(5)?,
        custom_cover: row.get(6)?,
        track_count: row.get(7)?,
        link_count: row.get(8)?,
        duration: row.get(9)?,
        added_at: row.get(10)?,
    })
}

/// Counts every track the artist is on (theirs, features, and their albums'); the picture
/// is the one the user chose, else one of their albums' covers, else a cover of a track
/// they lead, else of any.
pub(crate) const ARTIST_SELECT: &str = concat!(
    "SELECT ar.id, ar.name,
        (SELECT COUNT(*) FROM albums WHERE artist_id = ar.id),
        (SELECT COUNT(*) FROM (
            SELECT track_id FROM track_artists WHERE artist_id = ar.id
            UNION SELECT t.id FROM albums a JOIN tracks t ON t.album_id = a.id
                WHERE a.artist_id = ar.id)),
        COALESCE(
            ar.cover,
            (SELECT COALESCE(custom_cover, cover) FROM albums
                WHERE artist_id = ar.id AND COALESCE(custom_cover, cover) IS NOT NULL
                ORDER BY year DESC LIMIT 1),
            (SELECT ",
    track_cover!("t", "a"),
    " FROM track_artists ta
                JOIN tracks t ON t.id = ta.track_id LEFT JOIN albums a ON a.id = t.album_id
                WHERE ta.artist_id = ar.id AND ",
    track_cover!("t", "a"),
    " IS NOT NULL
                ORDER BY ta.position, t.added_at DESC LIMIT 1)),
        ar.cover IS NOT NULL
    FROM artists ar"
);

pub(crate) fn artist_from_row(row: &Row<'_>) -> rusqlite::Result<Artist> {
    Ok(Artist {
        id: row.get(0)?,
        name: row.get(1)?,
        album_count: row.get(2)?,
        track_count: row.get(3)?,
        cover: row.get(4)?,
        custom_cover: row.get(5)?,
    })
}

/// How albums are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSort {
    /// By album artist, then year.
    Artist,
    /// Newest additions first.
    RecentlyAdded,
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|v| !v.is_empty())
}

pub(crate) fn album_id(
    tx: &Transaction<'_>,
    title: Option<&str>,
    artist_id: Option<i64>,
    year: Option<u16>,
) -> Result<Option<i64>> {
    let Some(title) = non_empty(title) else {
        return Ok(None);
    };
    // UNIQUE treats NULL artists as distinct, so look up with `IS` before inserting.
    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM albums WHERE title = ?1 AND artist_id IS ?2",
            params![title, artist_id],
            |row| row.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            tx.execute(
                "INSERT INTO albums (title, artist_id) VALUES (?1, ?2)",
                params![title, artist_id],
            )?;
            tx.last_insert_rowid()
        }
    };
    if let Some(year) = year {
        tx.execute(
            "UPDATE albums SET year = ?2 WHERE id = ?1 AND year IS NULL",
            params![id, year],
        )?;
    }
    Ok(Some(id))
}

/// Inserts or updates a scanned file and its search entry; keeps play counts, favorites and
/// the date added (`added_at` is used for new rows only).
pub(crate) fn upsert_track(
    tx: &Transaction<'_>,
    folder_id: Option<i64>,
    track: &ScannedTrack,
    added_at: i64,
    splitting: &ArtistSplitting,
) -> Result<i64> {
    let m = &track.metadata;
    let (artists, credit) = artist_names(&m.artists, m.artist.as_deref(), splitting);
    let artist_ids = artists
        .iter()
        .map(|name| artist_id(tx, name))
        .collect::<Result<Vec<_>>>()?;
    // An album goes under its album artist's main artist, else the track's main artist, so
    // "A feat. B" doesn't make an album of its own.
    let album_artist = album_artist_name(non_empty(m.album_artist.as_deref()), splitting)
        .or_else(|| artists.first().cloned());
    let album_artist_id = album_artist
        .as_deref()
        .map(|name| artist_id(tx, name))
        .transpose()?;
    let album_id = album_id(tx, m.album.as_deref(), album_artist_id, m.year)?;
    let title = non_empty(m.title.as_deref())
        .map(str::to_owned)
        .unwrap_or_else(|| file_title(&track.path));

    let id: i64 = tx.query_row(
        "INSERT INTO tracks (path, folder_id, title, artist_id, album_id, album_artist, genre,
             year, track_number, disc_number, duration, format, sample_rate, bit_depth,
             bitrate, file_size, file_mtime, added_at, artist_credit)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
             ?19)
         ON CONFLICT (path) DO UPDATE SET
             folder_id = excluded.folder_id, title = excluded.title,
             artist_id = excluded.artist_id, album_id = excluded.album_id,
             artist_credit = excluded.artist_credit,
             album_artist = excluded.album_artist, genre = excluded.genre,
             year = excluded.year, track_number = excluded.track_number,
             disc_number = excluded.disc_number, duration = excluded.duration,
             format = excluded.format, sample_rate = excluded.sample_rate,
             bit_depth = excluded.bit_depth, bitrate = excluded.bitrate,
             file_size = excluded.file_size, file_mtime = excluded.file_mtime
         RETURNING id",
        params![
            track.path,
            folder_id,
            title,
            artist_ids.first(),
            album_id,
            non_empty(m.album_artist.as_deref()),
            non_empty(m.genre.as_deref()),
            m.year,
            m.track_number,
            m.disc_number,
            m.duration,
            m.format,
            m.sample_rate,
            m.bit_depth,
            m.bitrate,
            track.size as i64,
            track.mtime,
            added_at,
            credit,
        ],
        |row| row.get(0),
    )?;
    write_track_artists(tx, id, &artist_ids)?;

    // Lyrics saved in the app come first, as when they're shown.
    let saved: Option<String> = tx
        .query_row("SELECT text FROM lyrics WHERE track_id = ?", [id], |row| {
            row.get(0)
        })
        .optional()?;
    let words = saved
        .as_deref()
        .or(m.lyrics.as_deref())
        .map(searchable_lyrics);
    tx.execute("DELETE FROM track_search WHERE rowid = ?", [id])?;
    tx.execute(
        "INSERT INTO track_search (rowid, title, artist, album, album_artist, lyrics)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            fold(&title),
            (!artists.is_empty()).then(|| fold(&artists.join(" "))),
            non_empty(m.album.as_deref()).map(fold),
            album_artist.as_deref().map(fold),
            words
        ],
    )?;
    Ok(id)
}

/// Lyrics as searchable words: LRC timestamps and tags would only add noise to the index.
pub(crate) fn searchable_lyrics(text: &str) -> String {
    fold(&lyrics::parse(text).plain_text())
}

/// The file name without extension.
pub(crate) fn file_title(path: &str) -> String {
    let path = Path::new(path);
    path.file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn delete_tracks(tx: &Transaction<'_>, ids: &[i64]) -> Result<()> {
    for id in ids {
        tx.execute("DELETE FROM track_search WHERE rowid = ?", [id])?;
        tx.execute("DELETE FROM tracks WHERE id = ?", [id])?;
    }
    Ok(())
}

/// Drops albums and artists nothing refers to any more.
pub(crate) fn prune(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch(
        "DELETE FROM albums WHERE id NOT IN
             (SELECT album_id FROM tracks WHERE album_id IS NOT NULL);
         DELETE FROM artists WHERE id NOT IN
             (SELECT artist_id FROM track_artists
              UNION SELECT artist_id FROM albums WHERE artist_id IS NOT NULL);
         DELETE FROM artist_search WHERE rowid NOT IN (SELECT id FROM artists);",
    )?;
    Ok(())
}

impl Library {
    /// Stores scanned files; new ones count as added now.
    pub fn upsert_tracks(
        &mut self,
        folder_id: Option<i64>,
        tracks: &[ScannedTrack],
    ) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let added_at = now();
        let splitting = crate::artists::splitting(&tx)?;
        let ids = tracks
            .iter()
            .map(|track| upsert_track(&tx, folder_id, track, added_at, &splitting))
            .collect::<Result<_>>()?;
        tx.commit()?;
        Ok(ids)
    }

    pub fn tracks(&self, filter: &TrackFilter) -> Result<Vec<Track>> {
        let base = format!("SELECT {TRACK_COLUMNS} FROM {TRACK_FROM}");
        let query = |sql: String, args: Vec<i64>| -> Result<Vec<Track>> {
            let mut statement = self.conn.prepare(&sql)?;
            let rows = statement
                .query_map(params_from_iter(args), track_from_row)?
                .collect::<rusqlite::Result<_>>()?;
            Ok(rows)
        };
        match filter {
            TrackFilter::All => query(
                format!("{base} ORDER BY ar.name IS NULL, ar.name COLLATE NOCASE, {ALBUM_ORDER}"),
                vec![],
            ),
            TrackFilter::Album(id) => query(
                format!(
                    "{base} WHERE t.album_id = ?1 ORDER BY t.disc_number, t.track_number, t.title COLLATE NOCASE"
                ),
                vec![*id],
            ),
            TrackFilter::Artist(id) => query(
                format!(
                    "{base} WHERE t.artist_id = ?1 OR al.artist_id = ?1 ORDER BY {ALBUM_ORDER}"
                ),
                vec![*id],
            ),
            TrackFilter::ArtistAppearances(id) => query(
                format!(
                    "{base} WHERE t.id IN (SELECT track_id FROM track_artists WHERE artist_id = ?1)
                         AND t.artist_id IS NOT ?1 AND al.artist_id IS NOT ?1
                     ORDER BY ar.name COLLATE NOCASE, {ALBUM_ORDER}"
                ),
                vec![*id],
            ),
            TrackFilter::Favorites => query(
                format!(
                    "{base} WHERE t.favorite = 1 ORDER BY ar.name COLLATE NOCASE, {ALBUM_ORDER}"
                ),
                vec![],
            ),
            TrackFilter::RecentlyAdded { limit } => query(
                format!("{base} ORDER BY t.added_at DESC, t.id DESC LIMIT ?1"),
                vec![*limit],
            ),
            TrackFilter::Playlist(id) => query(
                format!(
                    "SELECT {TRACK_COLUMNS} FROM playlist_items pi, {TRACK_FROM}
                     WHERE t.id = pi.track_id AND pi.playlist_id = ?1 ORDER BY pi.position"
                ),
                vec![*id],
            ),
            TrackFilter::Ids(ids) => {
                let found: HashMap<i64, Track> = query(
                    format!("{base} WHERE t.id IN ({})", vec!["?"; ids.len()].join(",")),
                    ids.clone(),
                )?
                .into_iter()
                .map(|t| (t.id, t))
                .collect();
                Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
            }
        }
    }

    pub fn track(&self, id: i64) -> Result<Option<Track>> {
        Ok(self.tracks(&TrackFilter::Ids(vec![id]))?.into_iter().next())
    }

    pub fn track_by_path(&self, path: &str) -> Result<Option<Track>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TRACK_COLUMNS} FROM {TRACK_FROM} WHERE t.path = ?"),
                [path],
                track_from_row,
            )
            .optional()?)
    }

    pub fn albums(&self, sort: AlbumSort) -> Result<Vec<Album>> {
        let order = match sort {
            AlbumSort::Artist => {
                "ar.name IS NULL, ar.name COLLATE NOCASE, al.year, al.title COLLATE NOCASE"
            }
            AlbumSort::RecentlyAdded => "MAX(t.added_at) DESC, al.id DESC",
        };
        let mut statement = self
            .conn
            .prepare(&format!("{ALBUM_SELECT} GROUP BY al.id ORDER BY {order}"))?;
        let albums = statement
            .query_map([], album_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(albums)
    }

    pub fn album(&self, id: i64) -> Result<Option<Album>> {
        Ok(self
            .conn
            .query_row(
                &format!("{ALBUM_SELECT} WHERE al.id = ? GROUP BY al.id"),
                [id],
                album_from_row,
            )
            .optional()?)
    }

    /// Albums whose album artist is `artist_id`, oldest first.
    pub fn artist_albums(&self, artist_id: i64) -> Result<Vec<Album>> {
        let mut statement = self.conn.prepare(&format!(
            "{ALBUM_SELECT} WHERE al.artist_id = ? GROUP BY al.id
             ORDER BY al.year, al.title COLLATE NOCASE"
        ))?;
        let albums = statement
            .query_map([artist_id], album_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(albums)
    }

    pub fn artists(&self) -> Result<Vec<Artist>> {
        let mut statement = self
            .conn
            .prepare(&format!("{ARTIST_SELECT} ORDER BY ar.name COLLATE NOCASE"))?;
        let artists = statement
            .query_map([], artist_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(artists)
    }

    pub fn artist(&self, id: i64) -> Result<Option<Artist>> {
        Ok(self
            .conn
            .query_row(
                &format!("{ARTIST_SELECT} WHERE ar.id = ?"),
                [id],
                artist_from_row,
            )
            .optional()?)
    }

    pub fn set_favorite(&self, track_id: i64, favorite: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE tracks SET favorite = ?2 WHERE id = ?1",
            params![track_id, favorite],
        )?;
        Ok(())
    }

    /// Counts a play: bumps the play count and adds a history entry.
    pub fn record_play(&mut self, track_id: i64, at: i64) -> Result<()> {
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE tracks SET play_count = play_count + 1, last_played_at = ?2 WHERE id = ?1",
            params![track_id, at],
        )?;
        if updated > 0 {
            tx.execute(
                "INSERT INTO history (track_id, played_at) VALUES (?1, ?2)",
                params![track_id, at],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Most recent plays first.
    pub fn history(&self, limit: i64) -> Result<Vec<HistoryEntry>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {TRACK_COLUMNS}, h.played_at FROM history h, {TRACK_FROM}
             WHERE t.id = h.track_id
             ORDER BY h.played_at DESC, h.id DESC LIMIT ?"
        ))?;
        let entries = statement
            .query_map([limit], |row| {
                Ok(HistoryEntry {
                    track: track_from_row(row)?,
                    played_at: row.get(24)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(entries)
    }

    /// Sets (or clears) an album's cover thumbnail id.
    pub fn set_album_cover(&self, album_id: i64, cover: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE albums SET cover = ?2 WHERE id = ?1",
            params![album_id, cover],
        )?;
        Ok(())
    }

    /// Cover ids some album or track uses.
    pub fn cover_ids(&self) -> Result<HashSet<String>> {
        let mut statement = self.conn.prepare(
            "SELECT cover FROM albums WHERE cover IS NOT NULL
             UNION SELECT custom_cover FROM albums WHERE custom_cover IS NOT NULL
             UNION SELECT cover FROM artists WHERE cover IS NOT NULL
             UNION SELECT cover FROM playlists WHERE cover IS NOT NULL
             UNION SELECT cover FROM tracks WHERE cover IS NOT NULL",
        )?;
        let ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Paths of an album's tracks in album order, files only (no streams): the album's own
    /// files, then downloaded links (whose art is a site's thumbnail).
    pub fn album_paths(&self, album_id: i64) -> Result<Vec<String>> {
        Ok(self
            .album_files(album_id)?
            .into_iter()
            .map(|(path, _)| path)
            .collect())
    }

    /// [`Self::album_paths`], each marked whether it is a downloaded link.
    pub(crate) fn album_files(&self, album_id: i64) -> Result<Vec<(String, bool)>> {
        let mut statement = self.conn.prepare(
            "SELECT path, source_url IS NOT NULL FROM tracks
             WHERE album_id = ? AND path IS NOT source_url
             ORDER BY source_url IS NOT NULL, disc_number IS NULL, disc_number,
                 track_number IS NULL, track_number, title COLLATE NOCASE",
        )?;
        let paths = statement
            .query_map([album_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(paths)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ArtistSplitting;
    use media::TrackMetadata;

    pub(crate) fn scanned(
        path: &str,
        title: &str,
        artist: &str,
        album: &str,
        n: u32,
    ) -> ScannedTrack {
        ScannedTrack {
            path: path.into(),
            size: 100,
            mtime: 1,
            metadata: TrackMetadata {
                title: Some(title.into()),
                artist: Some(artist.into()),
                album: Some(album.into()),
                track_number: Some(n),
                duration: Some(200.0),
                year: Some(2020),
                ..TrackMetadata::default()
            },
        }
    }

    pub(crate) fn sample_library() -> Library {
        let mut library = Library::open_in_memory().unwrap();
        library
            .upsert_tracks(
                None,
                &[
                    scanned("/m/b/2.flac", "Second", "Boards", "Geogaddi", 2),
                    scanned("/m/b/1.flac", "First", "Boards", "Geogaddi", 1),
                    scanned("/m/a/1.mp3", "Aurora", "Аквариум", "Ихтиология", 1),
                    scanned("/m/c/x.mp3", "Café Song", "Amélie", "Paris", 1),
                ],
            )
            .unwrap();
        library
    }

    #[test]
    fn groups_tracks_into_albums_and_artists() {
        let library = sample_library();
        let albums = library.albums(AlbumSort::Artist).unwrap();
        assert_eq!(
            albums.iter().map(|a| a.title.as_str()).collect::<Vec<_>>(),
            ["Paris", "Geogaddi", "Ихтиология"]
        );
        let geogaddi = albums.iter().find(|a| a.title == "Geogaddi").unwrap();
        assert_eq!(geogaddi.track_count, 2);
        assert_eq!(geogaddi.duration, 400.0);
        assert_eq!(geogaddi.year, Some(2020));
        assert_eq!(library.artists().unwrap().len(), 3);
    }

    #[test]
    fn album_tracks_follow_track_numbers() {
        let library = sample_library();
        let album = library
            .albums(AlbumSort::Artist)
            .unwrap()
            .into_iter()
            .find(|a| a.title == "Geogaddi")
            .unwrap();
        let titles: Vec<_> = library
            .tracks(&TrackFilter::Album(album.id))
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["First", "Second"]);
    }

    #[test]
    fn rescanning_keeps_play_counts_and_favorites() {
        let mut library = sample_library();
        let track = library.track_by_path("/m/b/1.flac").unwrap().unwrap();
        library.set_favorite(track.id, true).unwrap();
        library.record_play(track.id, 1000).unwrap();

        library
            .upsert_tracks(
                None,
                &[scanned(
                    "/m/b/1.flac",
                    "First (Remaster)",
                    "Boards",
                    "Geogaddi",
                    1,
                )],
            )
            .unwrap();

        let updated = library.track(track.id).unwrap().unwrap();
        assert_eq!(updated.title, "First (Remaster)");
        assert!(updated.favorite);
        assert_eq!(updated.play_count, 1);
        assert_eq!(updated.added_at, track.added_at);
    }

    #[test]
    fn album_artist_groups_compilations() {
        let mut library = Library::open_in_memory().unwrap();
        let mut a = scanned("/v/1.mp3", "One", "Artist A", "Hits", 1);
        let mut b = scanned("/v/2.mp3", "Two", "Artist B", "Hits", 2);
        a.metadata.album_artist = Some("Various Artists".into());
        b.metadata.album_artist = Some("Various Artists".into());
        library.upsert_tracks(None, &[a, b]).unwrap();

        let albums = library.albums(AlbumSort::Artist).unwrap();
        assert_eq!(albums.len(), 1);
        assert_eq!(albums[0].artist.as_deref(), Some("Various Artists"));
        assert_eq!(albums[0].track_count, 2);
    }

    #[test]
    fn untitled_tracks_use_the_file_name() {
        let mut library = Library::open_in_memory().unwrap();
        let mut track = scanned("/x/03 - Intro.wav", "", "", "", 0);
        track.metadata = TrackMetadata::default();
        library.upsert_tracks(None, &[track]).unwrap();
        let stored = library.track_by_path("/x/03 - Intro.wav").unwrap().unwrap();
        assert_eq!(stored.title, "03 - Intro");
        assert_eq!(stored.album_id, None);
        assert!(library.albums(AlbumSort::Artist).unwrap().is_empty());
    }

    #[test]
    fn ids_filter_keeps_the_requested_order() {
        let library = sample_library();
        let all = library.tracks(&TrackFilter::All).unwrap();
        let wanted = vec![all[2].id, 999, all[0].id];
        let got: Vec<_> = library
            .tracks(&TrackFilter::Ids(wanted))
            .unwrap()
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(got, [all[2].id, all[0].id]);
    }

    #[test]
    fn records_history_newest_first() {
        let mut library = sample_library();
        let all = library.tracks(&TrackFilter::All).unwrap();
        library.record_play(all[0].id, 10).unwrap();
        library.record_play(all[1].id, 20).unwrap();
        library.record_play(all[0].id, 30).unwrap();

        let history = library.history(10).unwrap();
        assert_eq!(
            history
                .iter()
                .map(|h| (h.track.id, h.played_at))
                .collect::<Vec<_>>(),
            [(all[0].id, 30), (all[1].id, 20), (all[0].id, 10)]
        );
        assert_eq!(library.track(all[0].id).unwrap().unwrap().play_count, 2);
    }

    #[test]
    fn favorites_filter() {
        let library = sample_library();
        let track = library.track_by_path("/m/a/1.mp3").unwrap().unwrap();
        library.set_favorite(track.id, true).unwrap();
        let favorites = library.tracks(&TrackFilter::Favorites).unwrap();
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].id, track.id);
    }

    fn names(tracks: &[Track]) -> Vec<Vec<&str>> {
        tracks
            .iter()
            .map(|t| t.artists.iter().map(|a| a.name.as_str()).collect())
            .collect()
    }

    fn artist_named(library: &Library, name: &str) -> Artist {
        library
            .artists()
            .unwrap()
            .into_iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no artist {name}"))
    }

    #[test]
    fn a_credit_with_several_artists_makes_one_artist_each() {
        let mut library = Library::open_in_memory().unwrap();
        library
            .upsert_tracks(
                None,
                &[
                    scanned("/1.mp3", "Solo", "madk1d", "Single", 1),
                    scanned("/2.mp3", "Duo", "VILLIAN, madk1d", "Night", 1),
                    scanned("/3.mp3", "Trio", "greyrock, tewiq, madk1d", "Other", 1),
                    scanned("/4.mp3", "Prince", "madk1d & тёмный принц", "Night", 2),
                ],
            )
            .unwrap();
        let artists: Vec<String> = library
            .artists()
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(
            artists,
            ["greyrock", "madk1d", "tewiq", "VILLIAN", "тёмный принц"]
        );
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        let duo = tracks.iter().find(|t| t.title == "Duo").unwrap();
        assert_eq!(duo.artist.as_deref(), Some("VILLIAN, madk1d"));
        assert_eq!(names(std::slice::from_ref(duo)), [["VILLIAN", "madk1d"]]);
        assert_eq!(duo.artist_id, Some(duo.artists[0].id));

        // Albums go under the main artist: "Night" is VILLIAN's, though its second track
        // is led by madk1d.
        let night: Vec<Album> = library
            .albums(AlbumSort::Artist)
            .unwrap()
            .into_iter()
            .filter(|a| a.title == "Night")
            .collect();
        assert_eq!(
            night
                .iter()
                .map(|a| a.artist.as_deref())
                .collect::<Vec<_>>(),
            [Some("madk1d"), Some("VILLIAN")]
        );

        let madk1d = artist_named(&library, "madk1d");
        assert_eq!(madk1d.track_count, 4);
        let own = library.tracks(&TrackFilter::Artist(madk1d.id)).unwrap();
        assert_eq!(
            own.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
            ["Prince", "Solo"]
        );
        let features = library
            .tracks(&TrackFilter::ArtistAppearances(madk1d.id))
            .unwrap();
        assert_eq!(
            features
                .iter()
                .map(|t| t.title.as_str())
                .collect::<Vec<_>>(),
            ["Trio", "Duo"]
        );
        // Searching for the featured artist finds that artist alone.
        let found = library.search("madk", 10).unwrap();
        assert_eq!(
            found
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["madk1d"]
        );
        assert_eq!(found.tracks.len(), 4);
    }

    #[test]
    fn tag_lists_and_album_artists() {
        let mut library = Library::open_in_memory().unwrap();
        let mut listed = scanned("/1.flac", "One", "Tyler", "Album", 1);
        listed.metadata.artists = vec!["Tyler".into(), "The Creator".into()];
        let mut compilation = scanned("/2.flac", "Two", "A feat. B", "Hits", 1);
        compilation.metadata.album_artist = Some("Various Artists".into());
        let mut duo_album = scanned("/3.flac", "Three", "C", "Joint", 1);
        duo_album.metadata.album_artist = Some("VILLIAN, madk1d".into());
        library
            .upsert_tracks(None, &[listed, compilation, duo_album])
            .unwrap();
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        assert_eq!(
            names(&tracks),
            [vec!["A", "B"], vec!["C"], vec!["Tyler", "The Creator"]]
        );
        let albums = library.albums(AlbumSort::Artist).unwrap();
        let artists: Vec<_> = albums
            .iter()
            .map(|a| (a.title.as_str(), a.artist.as_deref()))
            .collect();
        assert!(artists.contains(&("Hits", Some("Various Artists"))));
        assert!(artists.contains(&("Joint", Some("VILLIAN"))));
        assert!(artists.contains(&("Album", Some("Tyler"))));
    }

    #[test]
    fn changing_the_splitting_setting_resplits_the_library() {
        let mut library = Library::open_in_memory().unwrap();
        let mut listed = scanned("/2.flac", "Listed", "X", "L", 1);
        listed.metadata.artists = vec!["X".into(), "Y, Z".into()];
        library
            .upsert_tracks(
                None,
                &[scanned("/1.mp3", "Song", "Foo & Bar", "Record", 1), listed],
            )
            .unwrap();
        assert!(library.artists().unwrap().iter().any(|a| a.name == "Foo"));

        let off = ArtistSplitting {
            enabled: false,
            exceptions: Vec::new(),
        };
        assert!(library.set_artist_splitting(&off).unwrap());
        assert!(!library.set_artist_splitting(&off).unwrap(), "unchanged");
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        assert_eq!(names(&tracks), [vec!["Foo & Bar"], vec!["X", "Y, Z"]]);
        // The album moved to the whole name, and the split names are gone.
        let album = library.album(tracks[0].album_id.unwrap()).unwrap().unwrap();
        assert_eq!(album.artist.as_deref(), Some("Foo & Bar"));
        assert!(library.artists().unwrap().iter().all(|a| a.name != "Foo"));
        assert_eq!(library.albums(AlbumSort::Artist).unwrap().len(), 2);
        assert_eq!(
            library.search("foo", 10).unwrap().artists[0].name,
            "Foo & Bar"
        );

        let with_exception = ArtistSplitting {
            enabled: true,
            exceptions: vec!["Foo & Bar".into()],
        };
        library.set_artist_splitting(&with_exception).unwrap();
        assert_eq!(library.artist_splitting().unwrap(), with_exception);
        let tracks = library.tracks(&TrackFilter::All).unwrap();
        assert_eq!(names(&tracks)[0], ["Foo & Bar"]);
    }

    #[test]
    fn deleting_tracks_prunes_empty_albums_and_artists() {
        let mut library = sample_library();
        let ids: Vec<i64> = library
            .tracks(&TrackFilter::All)
            .unwrap()
            .into_iter()
            .filter(|t| t.album.as_deref() == Some("Geogaddi"))
            .map(|t| t.id)
            .collect();
        let tx = library.conn.transaction().unwrap();
        delete_tracks(&tx, &ids).unwrap();
        prune(&tx).unwrap();
        tx.commit().unwrap();

        assert_eq!(library.albums(AlbumSort::Artist).unwrap().len(), 2);
        assert!(
            library
                .artists()
                .unwrap()
                .iter()
                .all(|a| a.name != "Boards")
        );
    }
}
