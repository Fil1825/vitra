//! Tracks and playlists from links. A streamed track is a single track (no folder) whose path
//! is its web page; downloading it later swaps the path for the file in the same row.

use std::fs;
use std::path::Path;

use media::TrackMetadata;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::artists::{ArtistSplitting, splitting};
use crate::model::{Playlist, RemoteTrack, ScannedTrack, Track};
use crate::playlists::{PLAYLIST_SELECT, playlist_from_row, write_items};
use crate::scan::{cache_cover, mtime, read_metadata};
use crate::tracks::{delete_tracks, prune, upsert_track};
use crate::{Error, Library, Result, now};

/// Stores a link as a track (or updates it); returns its id. A track already downloaded
/// from the link is returned as it is: its file's tags are what count now.
fn store_remote(
    tx: &Transaction<'_>,
    track: &RemoteTrack,
    at: i64,
    splitting: &ArtistSplitting,
) -> Result<i64> {
    // A downloaded copy comes first: playing a link the library has locally plays the file.
    let known: Option<(i64, String)> = tx
        .query_row(
            "SELECT id, path FROM tracks WHERE source_url = ?
             ORDER BY path = source_url LIMIT 1",
            [&track.url],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    match known {
        Some((id, path)) if path != track.url || !track.complete => return Ok(id),
        _ => {}
    }
    let scanned = ScannedTrack {
        path: track.url.clone(),
        size: 0,
        mtime: 0,
        metadata: TrackMetadata {
            title: Some(track.title.clone()),
            artist: track.artist.clone(),
            artists: track.artists.clone(),
            album: track.album.clone(),
            album_artist: track.album_artist.clone(),
            track_number: track.track_number,
            year: track.year,
            duration: track.duration,
            ..TrackMetadata::default()
        },
    };
    let id = upsert_track(tx, None, &scanned, at, splitting)?;
    tx.execute(
        "UPDATE tracks SET source_url = ?2 WHERE id = ?1",
        params![id, track.url],
    )?;
    Ok(id)
}

/// Moves everything that refers to track `from` over to `to`, then deletes `from`.
fn merge_track(tx: &Transaction<'_>, from: i64, to: i64) -> Result<()> {
    tx.execute(
        "UPDATE playlist_items SET track_id = ?2 WHERE track_id = ?1",
        params![from, to],
    )?;
    tx.execute(
        "UPDATE history SET track_id = ?2 WHERE track_id = ?1",
        params![from, to],
    )?;
    tx.execute(
        "INSERT OR IGNORE INTO lyrics (track_id, text, synced, source, updated_at)
         SELECT ?2, text, synced, source, updated_at FROM lyrics WHERE track_id = ?1",
        params![from, to],
    )?;
    tx.execute(
        "UPDATE tracks SET
             play_count = play_count + (SELECT play_count FROM tracks WHERE id = ?1),
             favorite = favorite OR (SELECT favorite FROM tracks WHERE id = ?1),
             last_played_at = NULLIF(MAX(COALESCE(last_played_at, 0),
                 COALESCE((SELECT last_played_at FROM tracks WHERE id = ?1), 0)), 0)
         WHERE id = ?2",
        params![from, to],
    )?;
    delete_tracks(tx, &[from])
}

impl Library {
    /// Adds links to the library as tracks (see [`RemoteTrack::complete`] for updates);
    /// returns their tracks in order.
    pub fn add_remote_tracks(&mut self, tracks: &[RemoteTrack]) -> Result<Vec<Track>> {
        let tx = self.conn.transaction()?;
        let at = now();
        let splitting = splitting(&tx)?;
        let ids = tracks
            .iter()
            .map(|track| store_remote(&tx, track, at, &splitting))
            .collect::<Result<Vec<_>>>()?;
        prune(&tx)?;
        tx.commit()?;
        self.tracks(&crate::TrackFilter::Ids(ids))
    }

    /// The track that came from `url`: its downloaded file if there is one, else the stream.
    pub fn track_by_source(&self, url: &str) -> Result<Option<Track>> {
        let id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM tracks WHERE source_url = ? ORDER BY path = source_url LIMIT 1",
                [url],
                |row| row.get(0),
            )
            .optional()?;
        match id {
            Some(id) => self.track(id),
            None => Ok(None),
        }
    }

    /// Caches `image` as the track's own cover; returns the cover id (`None` if the image
    /// can't be decoded).
    pub fn set_track_artwork(
        &self,
        track_id: i64,
        image: &[u8],
        covers_dir: &Path,
    ) -> Result<Option<String>> {
        let Some(cover) = cache_cover(image, covers_dir) else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE tracks SET cover = ?2 WHERE id = ?1",
            params![track_id, cover],
        )?;
        Ok(Some(cover))
    }

    /// Caches `image` as an album's cover unless the album has one; returns the cover id.
    pub fn set_album_artwork(
        &self,
        album_id: i64,
        image: &[u8],
        covers_dir: &Path,
    ) -> Result<Option<String>> {
        let current = self.album_artwork(album_id)?;
        if current.is_some() {
            return Ok(current);
        }
        let Some(cover) = cache_cover(image, covers_dir) else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE albums SET cover = ?2 WHERE id = ?1",
            params![album_id, cover],
        )?;
        Ok(Some(cover))
    }

    /// An album's automatic cover (not the one the user chose).
    fn album_artwork(&self, album_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT cover FROM albums WHERE id = ?", [album_id], |row| {
                row.get(0)
            })
            .optional()?
            .flatten())
    }

    /// Gives an album without a cover one made from a thumbnail of one of its tracks: its
    /// middle, cut square (YouTube's are 16:9 with the art in the middle). An album with a
    /// cover keeps it. Returns the album's cover id.
    pub fn cover_bare_album(
        &self,
        album_id: i64,
        thumbnail: &[u8],
        covers_dir: &Path,
    ) -> Result<Option<String>> {
        let current = self.album_artwork(album_id)?;
        if current.is_some() {
            return Ok(current);
        }
        match media::square_cover(thumbnail) {
            Ok(square) => self.set_album_artwork(album_id, &square, covers_dir),
            Err(err) => {
                log::info!("cannot make an album cover of a thumbnail: {err}");
                Ok(None)
            }
        }
    }

    /// Stores a link that enrichment described in full, with its thumbnail. A playlist entry
    /// gets the thumbnail as its own cover. An album entry gets none (its album's shows), but
    /// the first thumbnail covers the album if it has no cover yet: an album's listing often
    /// comes without a picture of its own.
    pub fn store_enriched(
        &mut self,
        track: &RemoteTrack,
        thumbnail: Option<&[u8]>,
        album_entry: bool,
        covers_dir: &Path,
    ) -> Result<Option<Track>> {
        let Some(stored) = self.add_remote_tracks(std::slice::from_ref(track))?.pop() else {
            return Ok(None);
        };
        // A downloaded track has its cover in the file.
        let streamed = stored.source_url.as_deref() == Some(stored.path.as_str());
        if let Some(image) = thumbnail.filter(|_| streamed) {
            match (album_entry, stored.album_id) {
                (true, Some(album_id)) => {
                    self.cover_bare_album(album_id, image, covers_dir)?;
                }
                (true, None) => {}
                (false, _) => {
                    self.set_track_artwork(stored.id, image, covers_dir)?;
                }
            }
        }
        self.track(stored.id)
    }

    /// The links of an album that has no cover (automatic or chosen), in album order: where
    /// to look for one. Empty for any other album.
    pub fn bare_link_album(&self, album_id: i64) -> Result<Vec<String>> {
        let bare: bool = self
            .conn
            .query_row(
                "SELECT cover IS NULL AND custom_cover IS NULL FROM albums WHERE id = ?",
                [album_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false);
        if !bare {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT path FROM tracks WHERE album_id = ? AND path IS source_url
             ORDER BY disc_number, track_number, title COLLATE NOCASE",
        )?;
        let links = statement
            .query_map([album_id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(links)
    }

    /// Imports a web playlist: its entries become tracks and fill a playlist. Importing the
    /// same link again updates that playlist's entries to the current listing and keeps its
    /// (possibly renamed) name.
    pub fn import_playlist(
        &mut self,
        name: &str,
        source_url: &str,
        tracks: &[RemoteTrack],
    ) -> Result<(Playlist, Vec<Track>)> {
        let tx = self.conn.transaction()?;
        let at = now();
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM playlists WHERE source_url = ? ORDER BY id LIMIT 1",
                [source_url],
                |row| row.get(0),
            )
            .optional()?;
        let playlist_id = match existing {
            Some(id) => id,
            None => {
                let name = name.trim();
                let name = if name.is_empty() { source_url } else { name };
                tx.execute(
                    "INSERT INTO playlists (name, created_at, updated_at, source_url)
                     VALUES (?1, ?2, ?2, ?3)",
                    params![name, at, source_url],
                )?;
                tx.last_insert_rowid()
            }
        };
        let splitting = splitting(&tx)?;
        let ids = tracks
            .iter()
            .map(|track| store_remote(&tx, track, at, &splitting))
            .collect::<Result<Vec<_>>>()?;
        write_items(&tx, playlist_id, &ids)?;
        prune(&tx)?;
        tx.commit()?;
        let playlist = self
            .playlist(playlist_id)?
            .ok_or_else(|| Error::Invalid("playlist vanished".into()))?;
        Ok((playlist, self.tracks(&crate::TrackFilter::Ids(ids))?))
    }

    /// The playlist imported from `url`, if any.
    pub fn playlist_by_source(&self, url: &str) -> Result<Option<Playlist>> {
        Ok(self
            .conn
            .query_row(
                &format!("{PLAYLIST_SELECT} WHERE p.source_url = ? GROUP BY p.id LIMIT 1"),
                [url],
                playlist_from_row,
            )
            .optional()?)
    }

    /// Puts a downloaded file in place of the streamed track `track_id`. The row stays, so
    /// playlists, lyrics, favorites and play counts carry over; its tags now come from the
    /// file, and it joins the library folder the file is in (if any). If a scan already
    /// added the file as a track of its own, that track is merged in. Without the stream's
    /// row (removed meanwhile), the file is added like an opened one.
    pub fn attach_download(
        &mut self,
        track_id: i64,
        path: &Path,
        covers_dir: &Path,
    ) -> Result<Track> {
        let key = path
            .to_str()
            .ok_or_else(|| Error::Invalid(format!("not a UTF-8 path: {}", path.display())))?;
        let file = fs::metadata(path)?;
        let metadata = read_metadata(path);
        let folder_id = self.folder_containing(path)?.map(|folder| folder.id);

        let tx = self.conn.transaction()?;
        let old_album: Option<Option<i64>> = tx
            .query_row(
                "SELECT album_id FROM tracks WHERE id = ?",
                [track_id],
                |row| row.get(0),
            )
            .optional()?;
        if old_album.is_some() {
            let scanned: Option<i64> = tx
                .query_row(
                    "SELECT id FROM tracks WHERE path = ?1 AND id <> ?2",
                    params![key, track_id],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(scanned) = scanned {
                merge_track(&tx, scanned, track_id)?;
            }
            tx.execute(
                "UPDATE tracks SET path = ?2 WHERE id = ?1",
                params![track_id, key],
            )?;
        }
        // Updates the row by its new path, so it keeps its id and date added.
        let splitting = splitting(&tx)?;
        let stored = upsert_track(
            &tx,
            folder_id,
            &ScannedTrack {
                path: key.to_owned(),
                size: file.len(),
                mtime: mtime(&file),
                metadata,
            },
            now(),
            &splitting,
        )?;
        let new_album: Option<i64> = tx.query_row(
            "SELECT album_id FROM tracks WHERE id = ?",
            [stored],
            |row| row.get(0),
        )?;
        prune(&tx)?;
        tx.commit()?;

        let albums: Vec<i64> = old_album.flatten().into_iter().chain(new_album).collect();
        self.refresh_covers(&albums, covers_dir)?;
        self.track(stored)?
            .ok_or_else(|| Error::Invalid("track vanished".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TrackFilter;
    use std::path::PathBuf;

    fn link(url: &str, title: &str, complete: bool) -> RemoteTrack {
        RemoteTrack {
            url: url.into(),
            title: title.into(),
            artist: Some("Uploader".into()),
            duration: Some(180.0),
            complete,
            ..RemoteTrack::default()
        }
    }

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../media/tests/fixtures")
            .join(name)
    }

    fn artwork() -> Vec<u8> {
        media::read_track(&fixture("tagged.flac"))
            .unwrap()
            .embedded_artwork
            .unwrap()
            .bytes
    }

    #[test]
    fn links_become_single_tracks_updated_only_by_complete_descriptions() {
        let mut library = Library::open_in_memory().unwrap();
        let url = "https://www.youtube.com/watch?v=a";
        let first = library
            .add_remote_tracks(&[link(url, "From a listing", false)])
            .unwrap();
        let track = &first[0];
        assert_eq!(track.path, url);
        assert_eq!(track.source_url.as_deref(), Some(url));
        assert_eq!(track.folder_id, None);
        assert_eq!(track.artist.as_deref(), Some("Uploader"));

        // Another listing doesn't overwrite; the track's own page does.
        library
            .add_remote_tracks(&[link(url, "Listed again", false)])
            .unwrap();
        assert_eq!(
            library.track(track.id).unwrap().unwrap().title,
            "From a listing"
        );
        let mut full = link(url, "Song", true);
        full.artist = Some("Artist".into());
        full.album = Some("Album".into());
        let updated = library.add_remote_tracks(&[full]).unwrap();
        assert_eq!(updated[0].id, track.id);
        assert_eq!(updated[0].title, "Song");
        assert_eq!(updated[0].album.as_deref(), Some("Album"));
        assert_eq!(library.tracks(&TrackFilter::All).unwrap().len(), 1);
        // The search index follows.
        assert_eq!(library.search_track_ids("song").unwrap(), [track.id]);
        // Removing a link works like removing any single track.
        assert_eq!(library.remove_single_tracks(&[track.id]).unwrap(), 1);
        assert!(library.artists().unwrap().is_empty());
    }

    #[test]
    fn imports_playlists_and_refreshes_them() {
        let mut library = Library::open_in_memory().unwrap();
        let list = "https://www.youtube.com/playlist?list=PL1";
        let (playlist, tracks) = library
            .import_playlist(
                "Mix",
                list,
                &[
                    link("https://y/1", "One", false),
                    link("https://y/2", "Two", false),
                ],
            )
            .unwrap();
        assert_eq!(playlist.name, "Mix");
        assert_eq!(playlist.source_url.as_deref(), Some(list));
        assert_eq!(playlist.track_count, 2);
        assert_eq!(playlist.duration, 360.0);
        assert_eq!(tracks.len(), 2);

        library.rename_playlist(playlist.id, "My mix").unwrap();
        let (again, _) = library
            .import_playlist(
                "Mix",
                list,
                &[
                    link("https://y/2", "Two", false),
                    link("https://y/3", "Three", false),
                ],
            )
            .unwrap();
        assert_eq!(again.id, playlist.id);
        assert_eq!(again.name, "My mix");
        let titles: Vec<String> = library
            .tracks(&TrackFilter::Playlist(playlist.id))
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(titles, ["Two", "Three"]);
        // The track that left the listing stays in the library.
        assert_eq!(library.tracks(&TrackFilter::All).unwrap().len(), 3);
        assert_eq!(
            library.playlist_by_source(list).unwrap().map(|p| p.id),
            Some(playlist.id)
        );
        assert_eq!(library.playlists().unwrap().len(), 1);
    }

    #[test]
    fn a_download_takes_over_the_stream_row() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let file = tmp.path().join("Artist - Song.flac");
        fs::copy(fixture("tagged.flac"), &file).unwrap();

        let mut library = Library::open_in_memory().unwrap();
        let url = "https://soundcloud.com/a/song";
        let stream = library
            .add_remote_tracks(&[link(url, "Song", true)])
            .unwrap()[0]
            .clone();
        library.set_favorite(stream.id, true).unwrap();
        library.record_play(stream.id, 100).unwrap();
        let playlist = library.create_playlist("P").unwrap();
        library
            .add_to_playlist(playlist.id, &[stream.id], None)
            .unwrap();

        let local = library.attach_download(stream.id, &file, &covers).unwrap();
        assert_eq!(local.id, stream.id);
        assert_eq!(local.path, file.to_str().unwrap());
        assert_eq!(local.source_url.as_deref(), Some(url));
        assert!(local.favorite);
        assert_eq!(local.play_count, 1);
        assert_eq!(local.added_at, stream.added_at);
        // Tags now come from the file (the fixture's), and so does the album cover.
        assert_ne!(local.title, "Song");
        assert_eq!(local.format.as_deref(), Some("FLAC"));
        assert!(local.cover.is_some());
        assert_eq!(
            library.tracks(&TrackFilter::Playlist(playlist.id)).unwrap()[0].id,
            stream.id
        );
        assert_eq!(library.history(10).unwrap()[0].track.id, stream.id);

        // The link now leads to the file, and listing it again doesn't add a stream.
        assert_eq!(library.track_by_source(url).unwrap().unwrap().id, stream.id);
        let listed = library
            .add_remote_tracks(&[link(url, "Song", true)])
            .unwrap();
        assert_eq!(listed[0].path, local.path);
        assert_eq!(library.tracks(&TrackFilter::All).unwrap().len(), 1);
    }

    #[test]
    fn a_download_found_by_a_scan_first_is_merged() {
        let tmp = tempfile::tempdir().unwrap();
        let music = tmp.path().join("Music");
        let covers = tmp.path().join("covers");
        fs::create_dir_all(&music).unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let folder = library.add_folder(music.to_str().unwrap()).unwrap();
        let url = "https://y/song";
        let stream = library
            .add_remote_tracks(&[link(url, "Song", true)])
            .unwrap()[0]
            .clone();
        library.record_play(stream.id, 50).unwrap();

        // The watcher's scan sees the new file before the download is attached.
        let file = music.join("Song.mp3");
        fs::copy(fixture("tagged.mp3"), &file).unwrap();
        library
            .scan_folder(folder.id, &covers, &Default::default(), &mut |_| {})
            .unwrap();
        let scanned = library
            .track_by_path(file.to_str().unwrap())
            .unwrap()
            .unwrap();
        library.set_favorite(scanned.id, true).unwrap();
        library.record_play(scanned.id, 60).unwrap();

        let local = library.attach_download(stream.id, &file, &covers).unwrap();
        assert_eq!(local.id, stream.id);
        assert_eq!(local.folder_id, Some(folder.id));
        assert!(local.favorite);
        assert_eq!(local.play_count, 2);
        assert_eq!(local.last_played_at, Some(60));
        assert_eq!(library.history(10).unwrap().len(), 2);
        assert_eq!(library.tracks(&TrackFilter::All).unwrap().len(), 1);
        assert_eq!(library.search_track_ids("polar").unwrap().len(), 1);
    }

    #[test]
    fn a_download_without_its_stream_is_added_as_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("x.wav");
        fs::copy(fixture("untagged.wav"), &file).unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let track = library
            .attach_download(42, &file, &tmp.path().join("covers"))
            .unwrap();
        assert_eq!(track.path, file.to_str().unwrap());
        assert_eq!(track.folder_id, None);
    }

    #[test]
    fn link_thumbnails_are_track_covers_kept_by_cleanup() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let mut song = link("https://y/1", "One", true);
        song.album = Some("Web Album".into());
        let track = library.add_remote_tracks(&[song]).unwrap()[0].clone();
        assert_eq!(track.cover, None);
        let cover = library
            .set_track_artwork(track.id, &artwork(), &covers)
            .unwrap()
            .unwrap();
        assert_eq!(
            library
                .set_track_artwork(track.id, b"not an image", &covers)
                .unwrap(),
            None
        );

        let track = library.track(track.id).unwrap().unwrap();
        assert_eq!(track.cover.as_deref(), Some(cover.as_str()));
        // An album of links shows a track's cover; its "files" are none.
        let album = library.album(track.album_id.unwrap()).unwrap().unwrap();
        assert_eq!(album.cover.as_deref(), Some(cover.as_str()));
        assert!(library.album_paths(album.id).unwrap().is_empty());
        let artist = library.artist(track.artist_id.unwrap()).unwrap().unwrap();
        assert_eq!(artist.cover.as_deref(), Some(cover.as_str()));

        assert_eq!(library.remove_unused_covers(&covers).unwrap(), 0);
        assert!(covers.join(format!("{cover}.jpg")).is_file());
        library.remove_single_tracks(&[track.id]).unwrap();
        assert_eq!(library.remove_unused_covers(&covers).unwrap(), 1);
    }

    #[test]
    fn album_links_make_one_album_with_its_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let track = |n: u32, artists: &[&str]| RemoteTrack {
            url: format!("https://y/{n}"),
            title: format!("Track {n}"),
            artist: Some(artists.join(", ")),
            artists: artists.iter().map(|a| (*a).to_owned()).collect(),
            album: Some("Polar Lights".into()),
            album_artist: Some("Aurora Collective".into()),
            track_number: Some(n),
            duration: Some(100.0),
            complete: false,
            ..RemoteTrack::default()
        };
        let stored = library
            .add_remote_tracks(&[
                track(1, &["Aurora Collective"]),
                track(2, &["Glass Harbor", "Aurora Collective"]),
            ])
            .unwrap();
        let album_id = stored[0].album_id.unwrap();
        assert_eq!(stored[1].album_id, Some(album_id), "one album, not singles");
        assert_eq!(stored[1].track_number, Some(2));
        let album = library.album(album_id).unwrap().unwrap();
        assert_eq!(album.artist.as_deref(), Some("Aurora Collective"));
        let cover = library
            .set_album_artwork(album_id, &artwork(), &covers)
            .unwrap()
            .unwrap();
        assert_eq!(
            library.album(album_id).unwrap().unwrap().cover.as_deref(),
            Some(cover.as_str())
        );
        // The tracks show it (they have no covers of their own).
        assert_eq!(
            library.track(stored[1].id).unwrap().unwrap().cover,
            Some(cover)
        );
        let glass = library
            .artists()
            .unwrap()
            .into_iter()
            .find(|a| a.name == "Glass Harbor")
            .unwrap();
        assert!(
            library
                .tracks(&TrackFilter::Artist(glass.id))
                .unwrap()
                .len()
                == 1
        );
    }

    /// An album link whose listing had no thumbnail: its entries' come with enrichment.
    #[test]
    fn an_album_without_a_cover_gets_one_from_its_first_described_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let entry = |n: u32, complete: bool| RemoteTrack {
            url: format!("https://y/{n}"),
            title: format!("Track {n}"),
            artist: Some("Sqwore".into()),
            album: Some("Summer".into()),
            album_artist: Some("Sqwore".into()),
            track_number: Some(n),
            complete,
            ..RemoteTrack::default()
        };
        // Stored from the listing, without a cover.
        let listed = library
            .add_remote_tracks(&[entry(1, false), entry(2, false)])
            .unwrap();
        let album_id = listed[0].album_id.unwrap();
        assert_eq!(library.album(album_id).unwrap().unwrap().cover, None);
        assert_eq!(
            library.bare_link_album(album_id).unwrap(),
            ["https://y/1", "https://y/2"]
        );

        // Enrichment: the first thumbnail covers the album, the tracks get none of their own.
        let first = library
            .store_enriched(&entry(1, true), Some(&artwork()), true, &covers)
            .unwrap()
            .unwrap();
        let cover = library.album(album_id).unwrap().unwrap().cover.unwrap();
        assert_eq!(first.cover.as_deref(), Some(cover.as_str()));
        let own: Option<String> = library
            .conn
            .query_row("SELECT cover FROM tracks WHERE id = ?", [first.id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(own, None);
        // Later thumbnails don't replace it.
        library
            .store_enriched(&entry(2, true), Some(b"not an image"), true, &covers)
            .unwrap();
        assert_eq!(
            library.album(album_id).unwrap().unwrap().cover,
            Some(cover.clone())
        );
        assert!(library.bare_link_album(album_id).unwrap().is_empty());
        // It is on disk, and in use.
        assert!(covers.join(format!("{cover}.jpg")).is_file());
        assert_eq!(library.remove_unused_covers(&covers).unwrap(), 0);

        // A playlist entry keeps its thumbnail as its own cover.
        let single = library
            .store_enriched(
                &link("https://y/9", "Nine", true),
                Some(&artwork()),
                false,
                &covers,
            )
            .unwrap()
            .unwrap();
        assert!(single.cover.is_some());
    }

    #[test]
    fn reads_saved_lyrics() {
        let mut library = Library::open_in_memory().unwrap();
        let track = library
            .add_remote_tracks(&[link("https://y/1", "One", true)])
            .unwrap()[0]
            .clone();
        assert_eq!(library.lyrics_text(track.id).unwrap(), None);
        library
            .conn
            .execute(
                "INSERT INTO lyrics (track_id, text, updated_at) VALUES (?1, 'la la', 0)",
                [track.id],
            )
            .unwrap();
        assert_eq!(
            library.lyrics_text(track.id).unwrap().as_deref(),
            Some("la la")
        );
    }
}
