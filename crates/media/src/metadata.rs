use std::path::Path;

use lofty::file::FileType;
use lofty::picture::{MimeType, PictureType};
use lofty::prelude::*;
use serde::Serialize;

use crate::MediaError;
use crate::artwork::Artwork;

/// Tags and audio properties shown in the UI. Every field is optional: files in the wild
/// carry anything from complete tags to nothing at all.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackMetadata {
    pub title: Option<String>,
    pub artist: Option<String>,
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
        let text = |value: Option<std::borrow::Cow<'_, str>>| {
            value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
        };
        metadata.title = text(tag.title());
        metadata.artist = text(tag.artist());
        metadata.album = text(tag.album());
        metadata.genre = text(tag.genre());
        metadata.album_artist = text(tag.get_string(ItemKey::AlbumArtist).map(Into::into));
        metadata.year = tag.date().map(|date| date.year);
        metadata.track_number = tag.track();
        metadata.disc_number = tag.disk();

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
}
