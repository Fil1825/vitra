//! Tracks and playlists from links. A streamed track is a single track (no folder) whose path
//! is its web page; downloading it later swaps the path for the file in the same row.

use std::fs;
use std::path::Path;

use media::TrackMetadata;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::artists::{ArtistSplitting, splitting};
use crate::model::{LinkEdit, Playlist, RemoteTrack, ScannedTrack, Track};
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
    let known: Option<(i64, String, bool)> = tx
        .query_row(
            "SELECT id, path, edited FROM tracks WHERE source_url = ?
             ORDER BY path = source_url LIMIT 1",
            [&track.url],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    match known {
        // What the user corrected stays, whatever yt-dlp says now.
        Some((id, path, edited)) if path != track.url || !track.complete || edited => {
            return Ok(id);
        }
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

    /// Corrects a streamed link's details (it has no file to tag). They stay when yt-dlp
    /// describes the link again; a download writes them into the file. The album moves the
    /// track: an existing album's title and album artist put it there, others make a new one.
    /// The track then shows its album's cover; an album without one gets the link's
    /// thumbnail, cut square: `thumbnail` if given (the full image), else the cached copy.
    pub fn edit_link(
        &mut self,
        url: &str,
        edit: &LinkEdit,
        thumbnail: Option<&[u8]>,
        covers_dir: &Path,
    ) -> Result<Track> {
        let current = self
            .track_by_path(url)?
            .filter(|track| track.source_url.as_deref() == Some(url))
            .ok_or_else(|| Error::Invalid(format!("no streamed link {url}")))?;
        let tx = self.conn.transaction()?;
        let splitting = splitting(&tx)?;
        // Several artists are taken as they are; one is a credit, split like a file's tag.
        let (artists, artist) = match edit.artists.as_slice() {
            [] => (Vec::new(), None),
            [one] => (Vec::new(), Some(one.clone())),
            several => (several.to_vec(), Some(several.join(", "))),
        };
        let scanned = ScannedTrack {
            path: url.to_owned(),
            size: 0,
            mtime: 0,
            metadata: TrackMetadata {
                title: Some(edit.title.trim().to_owned()).filter(|t| !t.is_empty()),
                artist,
                artists,
                album: edit.album.clone(),
                album_artist: edit.album_artist.clone(),
                year: edit.year,
                track_number: current.track_number.and_then(|n| u32::try_from(n).ok()),
                disc_number: current.disc_number.and_then(|n| u32::try_from(n).ok()),
                duration: current.duration,
                ..TrackMetadata::default()
            },
        };
        let id = upsert_track(&tx, None, &scanned, now(), &splitting)?;
        tx.execute(
            "UPDATE tracks SET source_url = ?2, edited = 1 WHERE id = ?1",
            params![id, url],
        )?;
        prune(&tx)?;
        tx.commit()?;
        let track = self
            .track(id)?
            .ok_or_else(|| Error::Invalid(format!("no streamed link {url}")))?;
        let own: Option<String> =
            self.conn
                .query_row("SELECT cover FROM tracks WHERE id = ?", [id], |row| {
                    row.get(0)
                })?;
        let (Some(album_id), Some(own)) = (track.album_id, own) else {
            return Ok(track);
        };
        if self.album_artwork(album_id)?.is_some() {
            return Ok(track);
        }
        let image = match thumbnail {
            Some(image) => image.to_vec(),
            None => match fs::read(covers_dir.join(format!("{own}.jpg"))) {
                Ok(image) => image,
                Err(err) => {
                    log::info!("cannot read the cover {own}: {err}");
                    return Ok(track);
                }
            },
        };
        self.cover_bare_album(album_id, &image, covers_dir)?;
        Ok(self.track(id)?.unwrap_or(track))
    }

    /// Whether the user corrected the streamed link `url` ([`Self::edit_link`]).
    pub fn link_edited(&self, url: &str) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT edited FROM tracks WHERE path = ?1 AND source_url = ?1",
                [url],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(false))
    }

    /// Gives a corrected link the details of its page back: `track` describes it afresh, in
    /// full. From then on yt-dlp's descriptions update it again.
    pub fn restore_link(&mut self, track: &RemoteTrack) -> Result<Track> {
        if !track.complete {
            return Err(Error::Invalid(format!(
                "a listing can't restore {}",
                track.url
            )));
        }
        let tx = self.conn.transaction()?;
        let found = tx.execute(
            "UPDATE tracks SET edited = 0 WHERE path = ?1 AND source_url = ?1",
            [&track.url],
        )?;
        if found == 0 {
            return Err(Error::Invalid(format!("no streamed link {}", track.url)));
        }
        let splitting = splitting(&tx)?;
        let id = store_remote(&tx, track, now(), &splitting)?;
        prune(&tx)?;
        tx.commit()?;
        self.track(id)?
            .ok_or_else(|| Error::Invalid(format!("no streamed link {}", track.url)))
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
    use crate::model::LinkEdit;
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

    /// A link moved into one of the user's albums and downloaded: the file carries the site's
    /// thumbnail, which must not replace the album's own cover.
    #[test]
    fn a_download_doesnt_replace_its_albums_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let album = tmp.path().join("Music/Fixtures");
        let covers = tmp.path().join("covers");
        fs::create_dir_all(&album).unwrap();
        // The album's own file has no embedded art; the folder has a cover.png.
        let own = album.join("01 One.flac");
        fs::copy(fixture("tagged.flac"), &own).unwrap();
        let values = media::TagValues {
            title: "One".into(),
            artist: "Vitra Test".into(),
            album: "Fixtures".into(),
            album_artist: "Various".into(),
            track: Some((1, None)),
            ..media::TagValues::default()
        };
        media::write_tags(&own, &values, &media::CoverChange::Remove).unwrap();
        fs::write(album.join("cover.png"), image(64, 64, [20, 90, 200])).unwrap();

        let mut library = Library::open_in_memory().unwrap();
        let folder = library
            .add_folder(tmp.path().join("Music").to_str().unwrap())
            .unwrap();
        library
            .scan_folder(folder.id, &covers, &Default::default(), &mut |_| {})
            .unwrap();
        let scanned = library
            .track_by_path(own.to_str().unwrap())
            .unwrap()
            .unwrap();
        let album_id = scanned.album_id.unwrap();
        let cover = library.album(album_id).unwrap().unwrap().cover;
        assert!(cover.is_some());

        // The download (same album, track 3, the fixture's own art) lands elsewhere.
        let stream = library
            .add_remote_tracks(&[link("https://y/song", "Song", true)])
            .unwrap()
            .remove(0);
        let file = tmp.path().join("Aurora.flac");
        fs::copy(fixture("tagged.flac"), &file).unwrap();
        let local = library.attach_download(stream.id, &file, &covers).unwrap();
        assert_eq!(local.album_id, Some(album_id));
        assert_eq!(library.album(album_id).unwrap().unwrap().cover, cover);
        // The album's own files come first.
        assert_eq!(
            library.album_paths(album_id).unwrap()[0],
            own.to_str().unwrap()
        );
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
        assert_eq!(
            album.link_count, 2,
            "both stream, so both can be downloaded"
        );
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

    /// A reupload: the channel stands in for the artist and the title is "dirty".
    #[test]
    fn a_corrected_link_keeps_its_details() {
        let tmp = tempfile::tempdir().unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let upload = RemoteTrack {
            url: "https://y/v1".into(),
            title: "Real Artist - Song (Official Video)".into(),
            artist: Some("Reupload Channel".into()),
            duration: Some(200.0),
            complete: true,
            ..RemoteTrack::default()
        };
        let stored = library
            .add_remote_tracks(std::slice::from_ref(&upload))
            .unwrap()
            .remove(0);
        let other = RemoteTrack {
            url: "https://y/v2".into(),
            title: "Other".into(),
            artist: Some("Real Artist".into()),
            album: Some("Real Album".into()),
            complete: true,
            ..RemoteTrack::default()
        };
        let existing = library.add_remote_tracks(&[other]).unwrap().remove(0);

        let edited = library
            .edit_link(
                &upload.url,
                &LinkEdit {
                    title: " Song ".into(),
                    artists: vec!["Real Artist".into(), "Guest".into()],
                    album: Some("Real Album".into()),
                    album_artist: None,
                    year: Some(2020),
                },
                None,
                tmp.path(),
            )
            .unwrap();
        assert_eq!(edited.id, stored.id);
        assert_eq!(edited.title, "Song");
        let names: Vec<_> = edited.artists.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Real Artist", "Guest"]);
        assert_eq!(
            edited.album_id, existing.album_id,
            "into the existing album"
        );
        assert_eq!(edited.duration, Some(200.0));
        assert_eq!(edited.year, Some(2020));
        // The channel isn't anyone's artist any more.
        let artists = library.artists().unwrap();
        assert!(artists.iter().all(|a| a.name != "Reupload Channel"));

        // yt-dlp describing it again, on its own or as a playlist entry, changes nothing.
        library
            .add_remote_tracks(std::slice::from_ref(&upload))
            .unwrap();
        library
            .store_enriched(&upload, None, false, tmp.path())
            .unwrap();
        let again = library.track(stored.id).unwrap().unwrap();
        assert_eq!(
            (again.title.as_str(), again.album_id),
            ("Song", existing.album_id)
        );

        // A new album name makes a new album; the old one keeps its other track.
        let moved = library
            .edit_link(
                &upload.url,
                &LinkEdit {
                    title: "Song".into(),
                    artists: vec!["Real Artist feat. Guest".into()],
                    album: Some("Singles".into()),
                    ..LinkEdit::default()
                },
                None,
                tmp.path(),
            )
            .unwrap();
        assert_ne!(moved.album_id, existing.album_id);
        assert_eq!(moved.album.as_deref(), Some("Singles"));
        // One credit is split like a file's tag.
        assert_eq!(moved.artists.len(), 2);
        assert!(library.album(existing.album_id.unwrap()).unwrap().is_some());

        // Files are the tag editor's.
        assert!(matches!(
            library.edit_link("/music/song.flac", &LinkEdit::default(), None, tmp.path()),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn a_corrected_link_takes_its_pages_details_back() {
        let tmp = tempfile::tempdir().unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let upload = RemoteTrack {
            url: "https://y/v1".into(),
            title: "Real Artist - Song (Official Video)".into(),
            artist: Some("Reupload Channel".into()),
            complete: true,
            ..RemoteTrack::default()
        };
        let stored = library
            .add_remote_tracks(std::slice::from_ref(&upload))
            .unwrap()
            .remove(0);
        assert!(!library.link_edited(&upload.url).unwrap());
        let edit = LinkEdit {
            title: "Song".into(),
            artists: vec!["Real Artist".into()],
            album: Some("Real Album".into()),
            ..LinkEdit::default()
        };
        library
            .edit_link(&upload.url, &edit, None, tmp.path())
            .unwrap();
        assert!(library.link_edited(&upload.url).unwrap());

        // The page as it is now.
        let page = RemoteTrack {
            title: "Real Artist - Song (Official Video) [HD]".into(),
            ..upload.clone()
        };
        let restored = library.restore_link(&page).unwrap();
        assert_eq!(restored.id, stored.id);
        assert_eq!(restored.title, page.title);
        assert_eq!(restored.artist.as_deref(), Some("Reupload Channel"));
        assert_eq!(restored.album, None);
        assert!(!library.link_edited(&upload.url).unwrap());
        // The album made for the correction went with it.
        assert!(library.albums(crate::AlbumSort::Artist).unwrap().is_empty());
        // And yt-dlp updates it again.
        let later = RemoteTrack {
            title: "Song (Remastered)".into(),
            ..upload.clone()
        };
        library.add_remote_tracks(&[later]).unwrap();
        assert_eq!(
            library.track(stored.id).unwrap().unwrap().title,
            "Song (Remastered)"
        );

        // A listing isn't a description, and files aren't links.
        let listing = RemoteTrack {
            complete: false,
            ..upload.clone()
        };
        assert!(library.restore_link(&listing).is_err());
        let file = RemoteTrack {
            url: "/music/song.flac".into(),
            ..upload
        };
        assert!(matches!(
            library.restore_link(&file),
            Err(Error::Invalid(_))
        ));
    }

    fn image(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, height, image::Rgb(rgb))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn a_link_put_into_an_album_shows_the_album_cover() {
        let tmp = tempfile::tempdir().unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let upload = library
            .add_remote_tracks(&[link("https://y/v1", "Song (Official Video)", true)])
            .unwrap()
            .remove(0);
        let thumbnail = library
            .set_track_artwork(upload.id, &image(160, 90, [200, 40, 90]), tmp.path())
            .unwrap()
            .unwrap();
        let other = library
            .add_remote_tracks(&[RemoteTrack {
                url: "https://y/v2".into(),
                title: "Other".into(),
                artist: Some("Artist".into()),
                album: Some("Album".into()),
                complete: true,
                ..RemoteTrack::default()
            }])
            .unwrap()
            .remove(0);
        let album_id = other.album_id.unwrap();
        let album_cover = library
            .set_album_artwork(album_id, &image(64, 64, [20, 90, 200]), tmp.path())
            .unwrap()
            .unwrap();
        let into = |album: &str| LinkEdit {
            title: "Song".into(),
            artists: vec!["Artist".into()],
            album: Some(album.into()),
            ..LinkEdit::default()
        };

        // An album with a cover: the track shows it, not its 16:9 thumbnail.
        let moved = library
            .edit_link("https://y/v1", &into("Album"), None, tmp.path())
            .unwrap();
        assert_eq!(moved.album_id, Some(album_id));
        assert_eq!(moved.cover.as_deref(), Some(album_cover.as_str()));

        // A new album gets the thumbnail, cut square, and the track shows that.
        let alone = library
            .edit_link("https://y/v1", &into("Singles"), None, tmp.path())
            .unwrap();
        let square = alone.cover.unwrap();
        assert_ne!(square, thumbnail);
        let album = library.album(alone.album_id.unwrap()).unwrap().unwrap();
        assert_eq!(album.cover.as_deref(), Some(square.as_str()));
        let (width, height) =
            image::image_dimensions(tmp.path().join(format!("{square}.jpg"))).unwrap();
        assert_eq!(width, height);

        // The full thumbnail, when the app still has it, gives a sharper one.
        let sharp = library
            .edit_link(
                "https://y/v1",
                &into("Big"),
                Some(&image(1280, 720, [200, 40, 90])),
                tmp.path(),
            )
            .unwrap();
        let cover = sharp.cover.unwrap();
        let size = image::image_dimensions(tmp.path().join(format!("{cover}.jpg"))).unwrap();
        assert_eq!(size, (crate::scan::COVER_SIZE, crate::scan::COVER_SIZE));

        // Without an album the thumbnail shows again; a link nobody corrected keeps its own.
        let bare = library
            .edit_link(
                "https://y/v1",
                &LinkEdit {
                    title: "Song".into(),
                    ..LinkEdit::default()
                },
                None,
                tmp.path(),
            )
            .unwrap();
        assert_eq!(bare.cover.as_deref(), Some(thumbnail.as_str()));
        library
            .set_track_artwork(other.id, &image(160, 90, [90, 200, 40]), tmp.path())
            .unwrap();
        let own = library.track(other.id).unwrap().unwrap().cover.unwrap();
        assert_ne!(own, album_cover);
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
