//! Writing tags: the basic tag editor (title, artist, album, album artist, year, cover) and
//! the lyrics editor.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::items::Timestamp;
use lofty::tag::{ItemValue, Tag, TagItem, TagType};
use serde::Deserialize;

use crate::MediaError;

/// What the tag editor saves. An empty string removes the field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagValues {
    pub title: String,
    /// Several artists are separated by ";" and written as separate values (see
    /// [`ARTIST_SEPARATOR`]).
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u16>,
    /// Track number and total; `None` leaves them as they are. Not in the tag editor;
    /// album downloads number their tracks.
    #[serde(skip)]
    pub track: Option<(u32, Option<u32>)>,
    /// Unsynced lyrics; `None` leaves them as they are. Not edited in the tag editor (the
    /// lyrics editor comes in stage 5); downloads write the lyrics saved in the app.
    #[serde(skip)]
    pub lyrics: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CoverChange {
    #[default]
    Keep,
    Remove,
    /// JPEG or PNG bytes.
    Replace(Vec<u8>),
}

/// Separates artists in [`TagValues::artist`].
pub const ARTIST_SEPARATOR: char = ';';

/// The artists in an artist field ("A; B").
pub fn artist_list(value: &str) -> Vec<&str> {
    value
        .split(ARTIST_SEPARATOR)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

/// Writes the artists as separate values where the format has them (Vorbis comments, ID3v2.4
/// `TPE1` with null separators). MP4's `©ART` takes one text in practice (Apple Music shows
/// only the first value), so there it holds "A, B" and the list goes to the `ARTISTS` atom,
/// as MusicBrainz Picard does.
fn set_artists(tag: &mut Tag, value: &str) {
    let names = artist_list(value);
    tag.remove_key(ItemKey::TrackArtist);
    tag.remove_key(ItemKey::TrackArtists);
    match names.as_slice() {
        [] => {}
        [name] => {
            tag.insert_text(ItemKey::TrackArtist, (*name).to_owned());
        }
        names if tag.tag_type() == TagType::Mp4Ilst => {
            tag.insert_text(ItemKey::TrackArtist, names.join(", "));
            for name in names {
                tag.push(TagItem::new(
                    ItemKey::TrackArtists,
                    ItemValue::Text((*name).to_owned()),
                ));
            }
        }
        names => {
            for name in names {
                tag.push(TagItem::new(
                    ItemKey::TrackArtist,
                    ItemValue::Text((*name).to_owned()),
                ));
            }
        }
    }
}

/// The hidden sibling the new tags are written to before it replaces the original.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(".");
    name.push(path.file_name().unwrap_or_default());
    name.push(".vitra-tmp");
    path.with_file_name(name)
}

fn set_text(tag: &mut Tag, key: ItemKey, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        tag.remove_key(key);
    } else {
        tag.insert_text(key, value.to_owned());
    }
}

/// The field plain (and LRC) lyrics go to. ID3v2 keeps them in USLT, which lofty only maps
/// from `UnsyncLyrics`; other formats' common field (Vorbis `LYRICS`, MP4 `©lyr`) is `Lyrics`.
fn lyrics_key(tag: &Tag) -> ItemKey {
    if tag.tag_type() == TagType::Id3v2 {
        ItemKey::UnsyncLyrics
    } else {
        ItemKey::Lyrics
    }
}

/// Edits the file's main tag (creating one if the file has none). The edit is written to a
/// copy that replaces the original only once it is complete, so a failed write never damages
/// it. `new_lyrics` also removes timed lyrics that would outrank the new text.
fn edit_tag(
    path: &Path,
    new_lyrics: bool,
    edit: impl FnOnce(&mut Tag) -> Result<(), MediaError>,
) -> Result<(), MediaError> {
    let mut file = lofty::read_from_path(path)?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let Some(tag) = file.primary_tag_mut() else {
        return Err(MediaError::Unsupported);
    };
    edit(tag)?;

    let temp = temp_path(path);
    let written = fs::copy(path, &temp)
        .map_err(MediaError::from)
        .and_then(|_| {
            file.save_to_path(&temp, WriteOptions::default())
                .map_err(MediaError::from)
        })
        // Old timed lyrics live outside the generic tag, so saving it keeps them.
        .and_then(|()| {
            if new_lyrics {
                crate::lyrics::clear_timed_lyrics(&temp)
            } else {
                Ok(())
            }
        })
        .and_then(|()| fs::rename(&temp, path).map_err(MediaError::from));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// Writes the tag editor's fields.
pub fn write_tags(path: &Path, values: &TagValues, cover: &CoverChange) -> Result<(), MediaError> {
    edit_tag(path, values.lyrics.is_some(), |tag| {
        set_text(tag, ItemKey::TrackTitle, &values.title);
        set_artists(tag, &values.artist);
        set_text(tag, ItemKey::AlbumTitle, &values.album);
        set_text(tag, ItemKey::AlbumArtist, &values.album_artist);
        if let Some((number, total)) = values.track {
            tag.set_track(number);
            match total {
                Some(total) => tag.set_track_total(total),
                None => tag.remove_track_total(),
            }
        }
        if let Some(lyrics) = &values.lyrics {
            set_text(tag, lyrics_key(tag), lyrics);
        }
        match values.year {
            Some(year) => tag.set_date(Timestamp {
                year,
                ..Timestamp::default()
            }),
            None => tag.remove_date(),
        }
        match cover {
            CoverChange::Keep => {}
            // Every picture goes: the reader falls back to any picture when there's no front
            // cover.
            CoverChange::Remove => {
                let types: Vec<PictureType> =
                    tag.pictures().iter().map(Picture::pic_type).collect();
                for picture_type in types {
                    tag.remove_picture_type(picture_type);
                }
            }
            CoverChange::Replace(bytes) => {
                let mime = match image::guess_format(bytes)? {
                    image::ImageFormat::Png => MimeType::Png,
                    image::ImageFormat::Jpeg => MimeType::Jpeg,
                    _ => return Err(MediaError::Unsupported),
                };
                tag.remove_picture_type(PictureType::CoverFront);
                tag.push_picture(
                    Picture::unchecked(bytes.clone())
                        .pic_type(PictureType::CoverFront)
                        .mime_type(mime)
                        .build(),
                );
            }
        }
        Ok(())
    })
}

/// Replaces the lyrics in a file's tags (plain text or LRC) and nothing else; an empty text
/// removes them. Timed frames (`SYLT`, `SYNCEDLYRICS`) go too, so the new text is what's read.
pub fn write_lyrics(path: &Path, text: &str) -> Result<(), MediaError> {
    edit_tag(path, true, |tag| {
        // A file may carry both fields (taggers differ); only one is written.
        tag.remove_key(ItemKey::Lyrics);
        tag.remove_key(ItemKey::UnsyncLyrics);
        set_text(tag, lyrics_key(tag), text);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::read_track;
    use image::{ImageFormat, RgbImage};
    use std::io::Cursor;

    fn copy_fixture(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let target = dir.join(name);
        fs::copy(source, &target).unwrap();
        target
    }

    fn jpeg() -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        RgbImage::from_pixel(8, 8, image::Rgb([10, 200, 90]))
            .write_to(&mut out, ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    fn values() -> TagValues {
        TagValues {
            title: "Новое имя".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            album_artist: String::new(),
            year: Some(1999),
            ..TagValues::default()
        }
    }

    #[test]
    fn writes_several_artists_as_separate_values() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["tagged.flac", "tagged.mp3", "tagged.opus", "tagged.m4a"] {
            let path = copy_fixture(dir.path(), name);
            let several = TagValues {
                artist: "VILLIAN; madk1d ;".into(),
                album_artist: "VILLIAN".into(),
                track: Some((3, Some(12))),
                ..values()
            };
            write_tags(&path, &several, &CoverChange::Keep).unwrap();
            let m = read_track(&path).unwrap().metadata;
            assert_eq!(m.artists, ["VILLIAN", "madk1d"], "{name}");
            assert_eq!(
                m.artist.as_deref(),
                Some(if name.ends_with(".m4a") {
                    "VILLIAN, madk1d"
                } else {
                    "VILLIAN"
                }),
                "{name}"
            );
            assert_eq!(m.track_number, Some(3), "{name}");
            assert_eq!(m.album_artist.as_deref(), Some("VILLIAN"), "{name}");

            // Back to one artist: no list left behind.
            write_tags(&path, &values(), &CoverChange::Keep).unwrap();
            let m = read_track(&path).unwrap().metadata;
            assert!(m.artists.is_empty(), "{name}");
            assert_eq!(m.artist.as_deref(), Some("Artist"), "{name}");
            assert_eq!(m.track_number, Some(3), "{name}: kept when not given");
        }
    }

    #[test]
    fn splits_artist_fields() {
        assert_eq!(artist_list(" A ;B;; "), ["A", "B"]);
        assert!(artist_list("").is_empty());
    }

    fn lyrics_of(path: &Path) -> Option<String> {
        let file = lofty::read_from_path(path).unwrap();
        let tag = file.primary_tag()?;
        tag.get_string(ItemKey::Lyrics)
            .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))
            .map(str::to_owned)
    }

    #[test]
    fn rewrites_fields_in_flac_and_mp3() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["tagged.flac", "tagged.mp3"] {
            let path = copy_fixture(dir.path(), name);
            write_tags(&path, &values(), &CoverChange::Keep).unwrap();
            let info = read_track(&path).unwrap();
            let m = info.metadata;
            assert_eq!(m.title.as_deref(), Some("Новое имя"), "{name}");
            assert_eq!(m.artist.as_deref(), Some("Artist"));
            assert_eq!(m.album.as_deref(), Some("Album"));
            assert_eq!(m.album_artist, None, "emptied fields are removed");
            assert_eq!(m.year, Some(1999));
            assert!(info.embedded_artwork.is_some(), "the cover was kept");
            assert!(m.duration.is_some(), "still a valid audio file");
        }
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".vitra-tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn writes_lyrics_only_when_given() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["tagged.flac", "tagged.mp3", "untagged.wav"] {
            let path = copy_fixture(dir.path(), name);
            let with_lyrics = TagValues {
                lyrics: Some("Первая строка\nSecond line".into()),
                ..values()
            };
            write_tags(&path, &with_lyrics, &CoverChange::Keep).unwrap();
            assert_eq!(
                lyrics_of(&path).as_deref(),
                Some("Первая строка\nSecond line"),
                "{name}"
            );
            write_tags(&path, &values(), &CoverChange::Keep).unwrap();
            assert!(lyrics_of(&path).is_some(), "{name}: kept when not given");
        }
    }

    #[test]
    fn write_lyrics_changes_only_the_lyrics() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["tagged.flac", "tagged.mp3", "untagged.wav"] {
            let path = copy_fixture(dir.path(), name);
            let before = read_track(&path).unwrap();
            write_lyrics(&path, "[00:01.00]Строка\n[00:02.00]Line").unwrap();
            assert_eq!(
                lyrics_of(&path).as_deref(),
                Some("[00:01.00]Строка\n[00:02.00]Line"),
                "{name}"
            );
            let after = read_track(&path).unwrap();
            assert_eq!(after.metadata.title, before.metadata.title, "{name}");
            assert_eq!(after.metadata.artist, before.metadata.artist, "{name}");
            assert_eq!(
                after.embedded_artwork.is_some(),
                before.embedded_artwork.is_some(),
                "{name}"
            );
            write_lyrics(&path, "  ").unwrap();
            assert_eq!(lyrics_of(&path), None, "{name}: removed");
        }
    }

    #[test]
    fn replaces_and_removes_the_cover() {
        let dir = tempfile::tempdir().unwrap();
        let path = copy_fixture(dir.path(), "tagged.flac");
        let cover = jpeg();
        write_tags(&path, &values(), &CoverChange::Replace(cover.clone())).unwrap();
        let art = read_track(&path).unwrap().embedded_artwork.unwrap();
        assert_eq!(art.bytes, cover);

        write_tags(&path, &values(), &CoverChange::Remove).unwrap();
        assert!(read_track(&path).unwrap().embedded_artwork.is_none());
    }

    #[test]
    fn tags_an_untagged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = copy_fixture(dir.path(), "untagged.wav");
        write_tags(&path, &values(), &CoverChange::Keep).unwrap();
        let m = read_track(&path).unwrap().metadata;
        assert_eq!(m.title.as_deref(), Some("Новое имя"));
        assert_eq!(m.year, Some(1999));
    }

    #[test]
    fn a_failed_write_leaves_the_file_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = copy_fixture(dir.path(), "tagged.mp3");
        let before = fs::read(&path).unwrap();
        let result = write_tags(
            &path,
            &values(),
            &CoverChange::Replace(b"not an image".to_vec()),
        );
        assert!(result.is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
