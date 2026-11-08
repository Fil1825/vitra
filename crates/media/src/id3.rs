//! ID3 text repair uses the frame encoding byte, not heuristics on every MP3 string.

use lofty::TextEncoding;
use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use std::borrow::Cow;

use lofty::id3::v2::{Frame, FrameId, TextInformationFrame};
use lofty::mpeg::MpegFile;
use lofty::tag::TagType;

use crate::MediaError;
use crate::encoding::try_repair_latin1_id3_text;
use crate::metadata::TrackMetadata;

/// Patches [TrackMetadata] when Latin-1 frames carry verified CP1251 repairs only.
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

    patch_latin1_text_frame(metadata, &tag, "TIT2", |m, v| m.title = Some(v));
    patch_tpe1(metadata, &tag);
    patch_latin1_text_frame(metadata, &tag, "TPE2", |m, v| m.album_artist = Some(v));
    patch_latin1_text_frame(metadata, &tag, "TALB", |m, v| m.album = Some(v));
    // Never patch TCON: Lofty resolves numeric genre ids (e.g. "(17)" -> Rock).
    Ok(())
}

fn patch_latin1_text_frame(
    metadata: &mut TrackMetadata,
    tag: &lofty::id3::v2::Id3v2Tag,
    frame_id: &str,
    apply: impl FnOnce(&mut TrackMetadata, String),
) {
    let id = FrameId::Valid(Cow::Borrowed(frame_id));
    let Some(Frame::Text(TextInformationFrame {
        encoding, value, ..
    })) = tag.get(&id)
    else {
        return;
    };
    if *encoding != TextEncoding::Latin1 {
        return;
    }
    let Some(repaired) = try_repair_latin1_id3_text(value.as_ref(), true) else {
        return;
    };
    if !repaired.is_empty() {
        apply(metadata, repaired);
    }
}

fn patch_tpe1(metadata: &mut TrackMetadata, tag: &lofty::id3::v2::Id3v2Tag) {
    let id = FrameId::Valid(Cow::Borrowed("TPE1"));
    let Some(Frame::Text(TextInformationFrame {
        encoding, value, ..
    })) = tag.get(&id)
    else {
        return;
    };
    if *encoding != TextEncoding::Latin1 {
        return;
    }
    let raw = value.as_ref();
    let Some(repaired) = try_repair_latin1_id3_text(raw, true) else {
        return;
    };
    let credits = split_tpe1_credits(&repaired);
    if credits.len() > 1 {
        metadata.artist = credits.first().cloned();
        metadata.artists = credits;
        return;
    }
    metadata.artist = Some(repaired.clone());
    if metadata.artists.iter().any(|a| a == raw) {
        if let Some(first) = metadata.artists.first_mut() {
            *first = repaired.clone();
        }
    } else if metadata.artists.len() == 1 {
        let sole = &metadata.artists[0];
        if sole == raw || metadata.artist.as_deref() == Some(raw) {
            metadata.artists[0] = repaired.clone();
        }
    }
}

fn split_tpe1_credits(value: &str) -> Vec<String> {
    let sep = value
        .contains('\0')
        .then_some('\0')
        .or_else(|| value.contains('/').then_some('/'))
        .or_else(|| value.contains(';').then_some(';'));
    let Some(sep) = sep else {
        return Vec::new();
    };
    value
        .split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// ID3v1 is always ISO-8859-1 in practice; apply repairs only when bytes change.
pub(crate) fn repair_id3v1_metadata(metadata: &mut TrackMetadata) {
    let repair = |value: &mut Option<String>| {
        if let Some(text) = value.take() {
            *value = try_repair_latin1_id3_text(&text, true).or(Some(text));
        }
    };
    repair(&mut metadata.title);
    repair(&mut metadata.artist);
    repair(&mut metadata.album);
    repair(&mut metadata.genre);
    repair(&mut metadata.album_artist);
    repair_id3v1_artists(metadata);
}

fn repair_id3v1_artists(metadata: &mut TrackMetadata) {
    if metadata.artists.is_empty() {
        return;
    }
    let mut changed = false;
    let updated: Vec<String> = metadata
        .artists
        .iter()
        .map(|name| {
            try_repair_latin1_id3_text(name, true)
                .inspect(|_| changed = true)
                .unwrap_or_else(|| name.clone())
        })
        .collect();
    if changed {
        metadata.artists = updated;
    }
}

pub(crate) fn is_id3v1(tag_type: TagType) -> bool {
    matches!(tag_type, TagType::Id3v1)
}
