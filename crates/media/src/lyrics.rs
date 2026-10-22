//! Lyrics stored in tags. Most formats keep them as text (ID3v2 `USLT`, Vorbis `LYRICS`, MP4
//! `©lyr`, APE `Lyrics`), often as LRC. Two places lofty's generic tag doesn't cover are read
//! from the concrete tags: ID3v2 `SYLT` frames and the Vorbis `SYNCEDLYRICS` field.

use std::fs::File;
use std::path::Path;

use std::borrow::Cow;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType};
use lofty::flac::FlacFile;
use lofty::id3::v2::{Frame, FrameId, Id3v2Tag, SynchronizedTextFrame, TimestampFormat};
use lofty::mpeg::MpegFile;
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::{OpusFile, VorbisFile};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::ItemKey;

use crate::MediaError;

/// What a file's tags hold in the way of lyrics.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TagLyrics {
    /// ID3v2 `SYLT`: (milliseconds, text) entries.
    pub sylt: Vec<(u32, String)>,
    /// Texts, best first: `SYNCEDLYRICS`, then the format's lyrics field(s). May be LRC.
    pub texts: Vec<String>,
}

impl TagLyrics {
    pub fn is_empty(&self) -> bool {
        self.sylt.is_empty() && self.texts.is_empty()
    }
}

fn push_text(texts: &mut Vec<String>, text: &str) {
    let text = text.trim();
    if !text.is_empty() && !texts.iter().any(|known| known == text) {
        texts.push(text.to_owned());
    }
}

fn sylt_entries(tag: &Id3v2Tag) -> Vec<(u32, String)> {
    tag.into_iter()
        .filter_map(|frame| match frame {
            Frame::Binary(binary) if binary.id().as_str() == "SYLT" => {
                SynchronizedTextFrame::parse(&binary.data, binary.flags()).ok()
            }
            _ => None,
        })
        // Timestamps in MPEG frames would need the frame rate; every tagger writes milliseconds.
        .filter(|sylt| sylt.timestamp_format == TimestampFormat::MS && !sylt.content.is_empty())
        .map(|sylt| sylt.content)
        .next()
        .unwrap_or_default()
}

fn vorbis_texts(comments: &VorbisComments, texts: &mut Vec<String>) {
    for key in ["SYNCEDLYRICS", "LYRICS", "UNSYNCEDLYRICS"] {
        for value in comments.get_all(key) {
            push_text(texts, value);
        }
    }
}

/// Removes timed lyrics that would outrank text just written to the lyrics field: ID3v2 `SYLT`
/// frames and the Vorbis `SYNCEDLYRICS` field (lofty's generic tag keeps the first, which isn't
/// part of it).
pub(crate) fn clear_timed_lyrics(path: &Path) -> Result<(), MediaError> {
    let file_type = Probe::open(path)?.guess_file_type()?.file_type();
    let options = ParseOptions::new().read_properties(false);
    let open = || File::open(path);
    let save = WriteOptions::default();
    match file_type {
        Some(FileType::Mpeg) => {
            let mut file = MpegFile::read_from(&mut open()?, options)?;
            let removed = file.id3v2_mut().map_or(0, |tag| {
                tag.remove(&FrameId::Valid(Cow::Borrowed("SYLT"))).count()
            });
            if removed > 0 {
                file.save_to_path(path, save)?;
            }
        }
        Some(FileType::Flac) => {
            let mut file = FlacFile::read_from(&mut open()?, options)?;
            let removed = file
                .vorbis_comments_mut()
                .map_or(0, |comments| comments.remove("SYNCEDLYRICS").count());
            if removed > 0 {
                file.save_to_path(path, save)?;
            }
        }
        Some(FileType::Opus) => {
            let mut file = OpusFile::read_from(&mut open()?, options)?;
            if file.vorbis_comments_mut().remove("SYNCEDLYRICS").count() > 0 {
                file.save_to_path(path, save)?;
            }
        }
        Some(FileType::Vorbis) => {
            let mut file = VorbisFile::read_from(&mut open()?, options)?;
            if file.vorbis_comments_mut().remove("SYNCEDLYRICS").count() > 0 {
                file.save_to_path(path, save)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reads the lyrics in a file's tags.
pub fn read_lyrics(path: &Path) -> Result<TagLyrics, MediaError> {
    let mut found = TagLyrics::default();
    let file_type = Probe::open(path)?.guess_file_type()?.file_type();
    let options = ParseOptions::new().read_properties(false);
    let open = || File::open(path);
    match file_type {
        Some(FileType::Mpeg) => {
            let file = MpegFile::read_from(&mut open()?, options)?;
            if let Some(tag) = file.id3v2() {
                found.sylt = sylt_entries(tag);
            }
        }
        Some(FileType::Flac) => {
            let file = FlacFile::read_from(&mut open()?, options)?;
            if let Some(comments) = file.vorbis_comments() {
                vorbis_texts(comments, &mut found.texts);
            }
        }
        Some(FileType::Opus) => {
            let file = OpusFile::read_from(&mut open()?, options)?;
            vorbis_texts(file.vorbis_comments(), &mut found.texts);
        }
        Some(FileType::Vorbis) => {
            let file = VorbisFile::read_from(&mut open()?, options)?;
            vorbis_texts(file.vorbis_comments(), &mut found.texts);
        }
        _ => {}
    }
    // Everything else (and USLT, ©lyr, APE) through lofty's generic view.
    let file = lofty::read_from_path(path)?;
    for tag in file.tags() {
        for key in [ItemKey::Lyrics, ItemKey::UnsyncLyrics] {
            for value in tag.get_strings(key) {
                push_text(&mut found.texts, value);
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoverChange, TagValues, write_tags};
    use lofty::TextEncoding;
    use lofty::id3::v2::{BinaryFrame, SyncTextContentType};
    use std::path::PathBuf;

    fn copy_fixture(dir: &Path, name: &str) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let target = dir.join(name);
        std::fs::copy(source, &target).unwrap();
        target
    }

    fn lyrics(text: &str) -> TagValues {
        TagValues {
            title: "T".into(),
            lyrics: Some(text.into()),
            ..TagValues::default()
        }
    }

    #[test]
    fn reads_lyrics_text_from_each_format() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["tagged.flac", "tagged.mp3", "untagged.wav"] {
            let path = copy_fixture(dir.path(), name);
            assert!(read_lyrics(&path).unwrap().is_empty(), "{name}");
            write_tags(&path, &lyrics("[00:01.00]Line"), &CoverChange::Keep).unwrap();
            let found = read_lyrics(&path).unwrap();
            assert_eq!(found.texts, ["[00:01.00]Line"], "{name}");
        }
    }

    #[test]
    fn reads_sylt_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = copy_fixture(dir.path(), "tagged.mp3");
        let sylt = SynchronizedTextFrame::new(
            TextEncoding::UTF8,
            *b"eng",
            TimestampFormat::MS,
            SyncTextContentType::Lyrics,
            None,
            vec![(1_000, "First".into()), (2_500, "Второй".into())],
        );
        let bytes = sylt.as_bytes(WriteOptions::default()).unwrap();
        let mut file =
            MpegFile::read_from(&mut File::open(&path).unwrap(), ParseOptions::new()).unwrap();
        let mut tag = file.id3v2_mut().cloned().unwrap_or_default();
        tag.insert(Frame::Binary(BinaryFrame::new(
            FrameId::Valid(Cow::Borrowed("SYLT")),
            bytes,
        )));
        file.set_id3v2(tag);
        file.save_to_path(&path, WriteOptions::default()).unwrap();

        let found = read_lyrics(&path).unwrap();
        assert_eq!(
            found.sylt,
            [(1_000, "First".to_owned()), (2_500, "Второй".to_owned())]
        );

        // Saving lyrics through the tag editor's path replaces the stale timed frame.
        write_tags(&path, &lyrics("New text"), &CoverChange::Keep).unwrap();
        let found = read_lyrics(&path).unwrap();
        assert!(found.sylt.is_empty());
        assert_eq!(found.texts, ["New text"]);
    }

    #[test]
    fn reads_synced_vorbis_lyrics_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = copy_fixture(dir.path(), "tagged.flac");
        let mut file =
            FlacFile::read_from(&mut File::open(&path).unwrap(), ParseOptions::new()).unwrap();
        let mut comments = file.vorbis_comments_mut().cloned().unwrap_or_default();
        comments.insert("LYRICS".into(), "plain words".into());
        comments.insert("SYNCEDLYRICS".into(), "[00:02.00]timed words".into());
        file.set_vorbis_comments(comments);
        file.save_to_path(&path, WriteOptions::default()).unwrap();

        let found = read_lyrics(&path).unwrap();
        assert_eq!(found.texts, ["[00:02.00]timed words", "plain words"]);

        write_tags(&path, &lyrics("[00:03.00]saved"), &CoverChange::Keep).unwrap();
        assert_eq!(read_lyrics(&path).unwrap().texts, ["[00:03.00]saved"]);
    }
}
