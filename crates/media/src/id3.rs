//! ID3 text repair uses the frame encoding byte, not heuristics on every MP3 string.

use lofty::TextEncoding;
use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use std::borrow::Cow;

use lofty::id3::v2::{Frame, FrameId, TextInformationFrame};
use lofty::mpeg::MpegFile;
use lofty::tag::TagType;

use crate::MediaError;
use crate::encoding::repair_latin1_id3_text;
use crate::metadata::TrackMetadata;

/// Patches [TrackMetadata] with ID3v2 frames that declare ISO-8859-1 / Latin-1.
pub(crate) fn enrich_mpeg_id3(
    path: &std::path::Path,
    metadata: &mut TrackMetadata,
) -> Result<(), MediaError> {
    use std::fs::File;
    use std::io::BufReader;

    let file = MpegFile::read_from(&mut BufReader::new(File::open(path)?), ParseOptions::new())?;
    let Some(tag) = file.id3v2().cloned() else {
        return Ok(());
    };
    const TEXT_FRAMES: &[(&str, fn(&mut TrackMetadata, String))] = &[
        ("TIT2", |m, v| m.title = Some(v)),
        ("TPE1", |m, v| m.artist = Some(v)),
        ("TPE2", |m, v| m.album_artist = Some(v)),
        ("TALB", |m, v| m.album = Some(v)),
        ("TCON", |m, v| m.genre = Some(v)),
    ];
    for (frame_id, apply) in TEXT_FRAMES {
        let id = FrameId::Valid(Cow::Borrowed(frame_id));
        let Some(Frame::Text(TextInformationFrame {
            encoding, value, ..
        })) = tag.get(&id)
        else {
            continue;
        };
        if *encoding != TextEncoding::Latin1 {
            continue;
        }
        let repaired = repair_latin1_id3_text(value.as_ref(), true);
        if repaired.is_empty() {
            continue;
        }
        apply(metadata, repaired);
    }
    Ok(())
}

/// ID3v1 is always ISO-8859-1 in practice; apply the same Latin-1-only repair.
pub(crate) fn repair_id3v1_metadata(metadata: &mut TrackMetadata) {
    let repair = |value: &mut Option<String>| {
        if let Some(text) = value.take() {
            *value = Some(repair_latin1_id3_text(&text, true)).filter(|s| !s.is_empty());
        }
    };
    repair(&mut metadata.title);
    repair(&mut metadata.artist);
    repair(&mut metadata.album);
    repair(&mut metadata.genre);
    repair(&mut metadata.album_artist);
}

pub(crate) fn is_id3v1(tag_type: TagType) -> bool {
    matches!(tag_type, TagType::Id3v1)
}
