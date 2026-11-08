use std::path::Path;

use lofty::file::FileType;
use lofty::picture::{MimeType, PictureType};
use lofty::prelude::*;
use serde::Serialize;

use crate::MediaError;
use crate::artwork::Artwork;
use crate::encoding::repair_id3_tag_text;
use lofty::tag::TagType;

/// Tags and audio properties shown in the UI. Every field is optional: files in the wild
/// carry anything from complete tags to nothing at all.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: Option<String>,
    /// The artist credit (the first artist value).
    pub artist: Option<String>,
    /// Every artist when the tags list several (multiple `ARTIST` values, an ID3v2.4 `TPE1`
    /// with null separators, an `ARTISTS` tag); empty otherwise.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u16>,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    /// Seconds.
    pub duration: Option<f64>,
    /// Container/codec family, e.g. "FLAC" or "MP3".
    pub format: Option<String>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    /// kbit/s.
    pub bitrate: Option<u32>,
    pub channels: Option<u8>,
    /// Lyrics from the tags' lyrics field (often LRC), for search. Not sent to the windows.
    #[serde(skip)]
    pub lyrics: Option<String>,
}

/// What `read_track` found in a file.
#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    pub metadata: TrackMetadata,
    pub embedded_artwork: Option<Artwork>,
}

/// Reads tags, audio properties and embedded artwork. Fails for formats lofty doesn't
/// understand (e.g. DSF, WMA); callers then fall back to what mpv reports.
pub fn read_track(path: &Path) -> Result<TrackInfo, MediaError> {
    let file = lofty::read_from_path(path)?;
    let properties = file.properties();
    let duration = properties.duration().as_secs_f64();

    let mut metadata = TrackMetadata {
        duration: (duration > 0.0).then_some(duration),
        format: format_name(file.file_type()).map(str::to_owned),
        sample_rate: properties.sample_rate(),
        bit_depth: properties.bit_depth(),
        bitrate: properties.audio_bitrate(),
        channels: properties.channels(),
        ..TrackMetadata::default()
    };

    let tag = file.primary_tag().or_else(|| file.first_tag());
    let mut embedded_artwork = None;
    if let Some(tag) = tag {
        let tag_type = tag.tag_type();
        let text = |value: Option<std::borrow::Cow<'_, str>>| {
            value
                .map(|v| repair_id3_tag_text(v.trim(), tag_type))
                .filter(|v| !v.is_empty())
        };
        metadata.title = text(tag.title());
        metadata.artist = text(tag.artist());
        metadata.artists = artist_values(tag, tag_type);
        metadata.album = text(tag.album());
        metadata.genre = text(tag.genre());
        metadata.album_artist = text(tag.get_string(ItemKey::AlbumArtist).map(Into::into));
        metadata.year = tag.date().map(|date| date.year);
        metadata.track_number = tag.track();
        metadata.disc_number = tag.disk();
        metadata.lyrics = text(
            tag.get_string(ItemKey::Lyrics)
                .or_else(|| tag.get_string(ItemKey::UnsyncLyrics))
                .map(Into::into),
        );

        let pictures = tag.pictures();
        let cover = pictures
            .iter()
            .find(|p| p.pic_type() == PictureType::CoverFront)
            .or_else(|| pictures.first());
        embedded_artwork = cover.map(|picture| Artwork {
            bytes: picture.data().to_vec(),
            mime: picture.mime_type().map(mime_name).map(str::to_owned),
        });
    }

    Ok(TrackInfo {
        metadata,
        embedded_artwork,
    })
}

/// The artists a tag lists, when it lists more than one: an `ARTISTS` tag (written by
/// MusicBrainz Picard and by Vitra for MP4) first, else several artist values.
pub(crate) fn artist_values(tag: &lofty::tag::Tag, tag_type: TagType) -> Vec<String> {
    let values = |key: ItemKey| -> Vec<String> {
        tag.get_strings(key)
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let listed: Vec<String> = values(ItemKey::TrackArtists)
        .into_iter()
        .map(|name| repair_id3_tag_text(&name, tag_type))
        .collect();
    if !listed.is_empty() {
        return listed;
    }
    let artists: Vec<String> = values(ItemKey::TrackArtist)
        .into_iter()
        .map(|name| repair_id3_tag_text(&name, tag_type))
        .collect();
    if artists.len() > 1 {
        artists
    } else {
        Vec::new()
    }
}

fn format_name(file_type: FileType) -> Option<&'static str> {
    Some(match file_type {
        FileType::Aac => "AAC",
        FileType::Aiff => "AIFF",
        FileType::Ape => "APE",
        FileType::Flac => "FLAC",
        FileType::Mpeg => "MP3",
        FileType::Mp4 => "MP4",
        FileType::Mpc => "Musepack",
        FileType::Opus => "Opus",
        FileType::Vorbis => "Vorbis",
        FileType::Speex => "Speex",
        FileType::Wav => "WAV",
        FileType::WavPack => "WavPack",
        _ => return None,
    })
}

fn mime_name(mime: &MimeType) -> &str {
    mime.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn reads_flac_tags_properties_and_cover() {
        let info = read_track(&fixture("tagged.flac")).unwrap();
        let m = &info.metadata;
        assert_eq!(m.title.as_deref(), Some("Aurora"));
        assert_eq!(m.artist.as_deref(), Some("Vitra Test"));
        assert_eq!(m.album.as_deref(), Some("Fixtures"));
        assert_eq!(m.album_artist.as_deref(), Some("Various"));
        assert_eq!(m.genre.as_deref(), Some("Ambient"));
        assert_eq!(m.year, Some(2024));
        assert_eq!(m.track_number, Some(3));
        assert_eq!(m.format.as_deref(), Some("FLAC"));
        assert_eq!(m.sample_rate, Some(8000));
        assert!((m.duration.unwrap() - 1.0).abs() < 0.05);

        let cover = info.embedded_artwork.unwrap();
        assert_eq!(cover.mime.as_deref(), Some("image/png"));
        assert!(cover.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn reads_id3_tags_and_cover() {
        let info = read_track(&fixture("tagged.mp3")).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Polar Night"));
        assert_eq!(info.metadata.year, Some(2023));
        assert_eq!(info.metadata.track_number, Some(7));
        assert_eq!(info.metadata.format.as_deref(), Some("MP3"));
        assert!(info.embedded_artwork.is_some());
    }

    #[test]
    fn untagged_files_still_report_properties() {
        let info = read_track(&fixture("untagged.wav")).unwrap();
        assert_eq!(info.metadata.title, None);
        assert_eq!(info.metadata.format.as_deref(), Some("WAV"));
        assert_eq!(info.metadata.channels, Some(1));
        assert!(info.embedded_artwork.is_none());
    }

    #[test]
    fn unreadable_files_are_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("noise.dsf");
        std::fs::write(&path, b"not really audio").unwrap();
        assert!(read_track(&path).is_err());
    }

    #[test]
    fn read_track_repairs_latin1_id3_cp1251_bytes() {
        use std::borrow::Cow;
        use std::fs::File;
        use std::io::BufReader;

        use lofty::TextEncoding;
        use lofty::config::{ParseOptions, WriteOptions};
        use lofty::file::AudioFile;
        use lofty::id3::v2::{Frame, FrameId, TextInformationFrame};
        use lofty::mpeg::MpegFile;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cp1251.mp3");
        std::fs::copy(fixture("tagged.mp3"), &path).unwrap();

        let album = "Ýêñêëþçèâ";
        let artist = "09.Íîããàíî";
        let title = "íó êàê òà êàê";
        let mut file = MpegFile::read_from(
            &mut BufReader::new(File::open(&path).unwrap()),
            ParseOptions::new(),
        )
        .unwrap();
        let mut tag = file.id3v2_mut().cloned().unwrap_or_default();
        for (id, value) in [("TIT2", title), ("TPE1", artist), ("TALB", album)] {
            tag.insert(Frame::Text(TextInformationFrame::new(
                FrameId::Valid(Cow::Borrowed(id)),
                TextEncoding::Latin1,
                value,
            )));
        }
        file.set_id3v2(tag);
        file.save_to_path(&path, WriteOptions::default()).unwrap();

        let info = read_track(&path).unwrap();
        assert_eq!(info.metadata.album.as_deref(), Some("Эксклюзив"));
        assert_eq!(info.metadata.artist.as_deref(), Some("09.Ноггано"));
        assert!(
            info.metadata
                .title
                .as_deref()
                .is_some_and(|t| t.chars().any(is_cyrillic_char))
        );
    }

    fn is_cyrillic_char(c: char) -> bool {
        matches!(c, '\u{0400}'..='\u{04FF}')
    }
}
