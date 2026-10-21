//! Writing tags: the basic tag editor (title, artist, album, album artist, year, cover).

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::Tag;
use lofty::tag::items::Timestamp;
use serde::Deserialize;

use crate::MediaError;

/// What the tag editor saves. An empty string removes the field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagValues {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum CoverChange {
    #[default]
    Keep,
    Remove,
    /// JPEG or PNG bytes.
    Replace(Vec<u8>),
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

/// Writes tags into the file's main tag (creating one if the file has none). The original
/// is replaced only once the new file is complete, so a failed write never damages it.
pub fn write_tags(path: &Path, values: &TagValues, cover: &CoverChange) -> Result<(), MediaError> {
    let mut file = lofty::read_from_path(path)?;
    if file.primary_tag().is_none() {
        file.insert_tag(Tag::new(file.primary_tag_type()));
    }
    let Some(tag) = file.primary_tag_mut() else {
        return Err(MediaError::Unsupported);
    };

    set_text(tag, ItemKey::TrackTitle, &values.title);
    set_text(tag, ItemKey::TrackArtist, &values.artist);
    set_text(tag, ItemKey::AlbumTitle, &values.album);
    set_text(tag, ItemKey::AlbumArtist, &values.album_artist);
    match values.year {
        Some(year) => tag.set_date(Timestamp {
            year,
            ..Timestamp::default()
        }),
        None => tag.remove_date(),
    }
    match cover {
        CoverChange::Keep => {}
        // Every picture goes: the reader falls back to any picture when there's no front cover.
        CoverChange::Remove => {
            let types: Vec<PictureType> = tag.pictures().iter().map(Picture::pic_type).collect();
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

    let temp = temp_path(path);
    let written = fs::copy(path, &temp)
        .map_err(MediaError::from)
        .and_then(|_| {
            file.save_to_path(&temp, WriteOptions::default())
                .map_err(MediaError::from)
        })
        .and_then(|()| fs::rename(&temp, path).map_err(MediaError::from));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
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
        }
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
