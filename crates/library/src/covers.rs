//! Covers the user chose for albums, artists and playlists. They are stored apart from the
//! automatic ones (`albums.custom_cover`, `artists.cover`, `playlists.cover`), so scans and
//! links never replace them, and every query shows them first.

use std::path::Path;

use rusqlite::{OptionalExtension, params};
use serde::Deserialize;

use crate::scan::{cover_id, write_cover};
use crate::{Error, Library, Result};

/// Custom covers are kept larger than automatic ones (512 px): Now Playing and the full-screen
/// view show them too.
const CUSTOM_COVER_SIZE: u32 = 1200;

/// What a cover is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "camelCase")]
pub enum CoverTarget {
    Album(i64),
    Artist(i64),
    Playlist(i64),
}

impl CoverTarget {
    /// Table, column and row id the cover goes into.
    fn column(self) -> (&'static str, &'static str, i64) {
        match self {
            Self::Album(id) => ("albums", "custom_cover", id),
            Self::Artist(id) => ("artists", "cover", id),
            Self::Playlist(id) => ("playlists", "cover", id),
        }
    }
}

/// A custom cover from an image file: its centre cut square (artists show it round), at most
/// [`CUSTOM_COVER_SIZE`] wide, as JPEG.
fn custom_cover_image(bytes: &[u8]) -> Result<Vec<u8>> {
    let not_an_image = |err: media::MediaError| Error::Invalid(format!("not an image: {err}"));
    let square = media::square_cover(bytes).map_err(not_an_image)?;
    media::cover_thumbnail(&square, CUSTOM_COVER_SIZE).map_err(not_an_image)
}

impl Library {
    /// Sets the cover the user chose (the bytes of an image file), or with `None` goes back
    /// to the automatic one; returns the new cover id.
    pub fn set_custom_cover(
        &self,
        target: CoverTarget,
        image: Option<&[u8]>,
        covers_dir: &Path,
    ) -> Result<Option<String>> {
        let (table, column, id) = target.column();
        let cover = match image {
            Some(bytes) => {
                let image = custom_cover_image(bytes)?;
                // Named apart from automatic covers, which are smaller: picking the image an
                // album already has must not show its 512 px copy.
                let key = cover_id(&[b"custom:".as_slice(), bytes].concat());
                let cover = write_cover(key, covers_dir, || Some(image))
                    .ok_or_else(|| Error::Invalid("cannot save the cover".into()))?;
                Some(cover)
            }
            None => None,
        };
        let changed = self.conn.execute(
            &format!("UPDATE {table} SET {column} = ?2 WHERE id = ?1"),
            params![id, cover],
        )?;
        if changed == 0 {
            return Err(Error::Invalid(format!(
                "nothing to give a cover: {target:?}"
            )));
        }
        Ok(cover)
    }

    /// The cover the user chose for the album of the track at `path`: shown before the
    /// artwork in the file.
    pub fn custom_album_cover(&self, path: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT al.custom_cover FROM tracks t JOIN albums al ON al.id = t.album_id
                 WHERE t.path = ?",
                [path],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }

    /// The cover the user chose for an album.
    pub fn custom_cover_of_album(&self, album_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT custom_cover FROM albums WHERE id = ?",
                [album_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RemoteTrack;

    /// A plain PNG of the given size.
    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(width, height, image::Rgb([200, 40, 90]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        bytes.into_inner()
    }

    fn size(path: &Path) -> (u32, u32) {
        image::image_dimensions(path).unwrap()
    }

    fn link(n: u32) -> RemoteTrack {
        RemoteTrack {
            url: format!("https://y/{n}"),
            title: format!("Track {n}"),
            artist: Some("Aurora Collective".into()),
            album: Some("Polar Lights".into()),
            track_number: Some(n),
            complete: false,
            ..RemoteTrack::default()
        }
    }

    #[test]
    fn a_custom_album_cover_comes_first_and_outlives_automatic_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let tracks = library.add_remote_tracks(&[link(1), link(2)]).unwrap();
        let album_id = tracks[0].album_id.unwrap();
        let automatic = library
            .set_album_artwork(album_id, &png(300, 300), &covers)
            .unwrap()
            .unwrap();

        let custom = library
            .set_custom_cover(CoverTarget::Album(album_id), Some(&png(300, 300)), &covers)
            .unwrap()
            .unwrap();
        assert_ne!(custom, automatic, "the same image, but kept apart");
        let album = library.album(album_id).unwrap().unwrap();
        assert_eq!(album.cover.as_deref(), Some(custom.as_str()));
        assert!(album.custom_cover);
        // Its tracks, its artist and a playlist of them show it too.
        let track = library.track(tracks[0].id).unwrap().unwrap();
        assert_eq!(track.cover.as_deref(), Some(custom.as_str()));
        let artist = library.artist(track.artist_id.unwrap()).unwrap().unwrap();
        assert_eq!(artist.cover.as_deref(), Some(custom.as_str()));
        assert!(!artist.custom_cover);
        let playlist = library.create_playlist("Mix").unwrap();
        library
            .add_to_playlist(playlist.id, &[tracks[1].id], None)
            .unwrap();
        let playlist = library.playlist(playlist.id).unwrap().unwrap();
        assert_eq!(playlist.cover.as_deref(), Some(custom.as_str()));
        assert!(!playlist.custom_cover);
        assert_eq!(
            library.custom_album_cover(&track.path).unwrap().as_deref(),
            Some(custom.as_str())
        );

        // Links stored again and covers found for them leave it alone.
        library.add_remote_tracks(&[link(1), link(2)]).unwrap();
        library
            .cover_bare_album(album_id, &png(160, 90), &covers)
            .unwrap();
        assert_eq!(
            library.album(album_id).unwrap().unwrap().cover.as_deref(),
            Some(custom.as_str())
        );

        // Reset: the automatic one is back.
        let reset = library
            .set_custom_cover(CoverTarget::Album(album_id), None, &covers)
            .unwrap();
        assert_eq!(reset, None);
        let album = library.album(album_id).unwrap().unwrap();
        assert_eq!(album.cover.as_deref(), Some(automatic.as_str()));
        assert!(!album.custom_cover);
        assert_eq!(library.custom_album_cover(&track.path).unwrap(), None);
    }

    #[test]
    fn artists_and_playlists_take_a_square_picture_and_can_drop_it() {
        let tmp = tempfile::tempdir().unwrap();
        let covers = tmp.path().join("covers");
        let mut library = Library::open_in_memory().unwrap();
        let tracks = library.add_remote_tracks(&[link(1)]).unwrap();
        let artist_id = tracks[0].artist_id.unwrap();
        let playlist = library.create_playlist("Mix").unwrap();
        library
            .add_to_playlist(playlist.id, &[tracks[0].id], None)
            .unwrap();
        assert_eq!(library.playlist(playlist.id).unwrap().unwrap().cover, None);

        let photo = library
            .set_custom_cover(CoverTarget::Artist(artist_id), Some(&png(64, 32)), &covers)
            .unwrap()
            .unwrap();
        let artist = library.artist(artist_id).unwrap().unwrap();
        assert_eq!(artist.cover.as_deref(), Some(photo.as_str()));
        assert!(artist.custom_cover);
        assert_eq!(
            size(&covers.join(format!("{photo}.jpg"))),
            (32, 32),
            "cut square from the middle"
        );
        let found = library.search("Aurora", 10).unwrap();
        assert_eq!(found.artists[0].cover.as_deref(), Some(photo.as_str()));

        let big = library
            .set_custom_cover(
                CoverTarget::Playlist(playlist.id),
                Some(&png(2000, 2000)),
                &covers,
            )
            .unwrap()
            .unwrap();
        let shown = library.playlist(playlist.id).unwrap().unwrap();
        assert_eq!(shown.cover.as_deref(), Some(big.as_str()));
        assert!(shown.custom_cover);
        assert_eq!(size(&covers.join(format!("{big}.jpg"))), (1200, 1200));

        // Cleaning up the covers folder keeps what is in use, and only that.
        assert_eq!(library.remove_unused_covers(&covers).unwrap(), 0);
        library
            .set_custom_cover(CoverTarget::Artist(artist_id), None, &covers)
            .unwrap();
        library
            .set_custom_cover(CoverTarget::Playlist(playlist.id), None, &covers)
            .unwrap();
        assert!(!library.artist(artist_id).unwrap().unwrap().custom_cover);
        assert_eq!(library.playlist(playlist.id).unwrap().unwrap().cover, None);
        assert_eq!(library.remove_unused_covers(&covers).unwrap(), 2);
    }

    #[test]
    fn what_is_not_an_image_or_not_there_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut library = Library::open_in_memory().unwrap();
        let tracks = library.add_remote_tracks(&[link(1)]).unwrap();
        let album = CoverTarget::Album(tracks[0].album_id.unwrap());
        assert!(matches!(
            library.set_custom_cover(album, Some(b"text"), tmp.path()),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            library.set_custom_cover(CoverTarget::Artist(999), Some(&png(4, 4)), tmp.path()),
            Err(Error::Invalid(_))
        ));
    }
}
