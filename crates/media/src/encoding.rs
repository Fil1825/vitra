//! Legacy tag text: ID3v2 often marks Cyrillic as ISO-8859-1 (encoding byte `0`) while the
//! payload is Windows-1251. Lofty decodes those bytes as Latin-1, which yields mojibake.

use lyrics::decode_text;

/// Bump when tag decoding logic changes so the library scanner re-reads affected tracks.
pub const TAG_DECODER_VERSION: i64 = 2;

/// Corrects likely Windows-1251-in-ISO-8859-1 mis-decoding when the frame used Latin-1.
pub fn repair_latin1_id3_text(text: &str, id3_latin1: bool) -> String {
    if text.is_empty() || !id3_latin1 {
        return text.to_owned();
    }
    let Some(bytes) = latin1_bytes(text) else {
        return text.to_owned();
    };
    if !likely_cp1251_latin1_mojibake(&bytes) {
        return text.to_owned();
    }
    let repaired = decode_text(&bytes);
    if repaired == text {
        return text.to_owned();
    }
    let cyrillic = repaired.chars().filter(|c| is_cyrillic(*c)).count();
    let latin_letters = text.chars().filter(|c| c.is_ascii_alphabetic()).count();
    if cyrillic >= 2 && cyrillic * 2 > latin_letters + cyrillic {
        repaired
    } else {
        text.to_owned()
    }
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}')
}

/// A high byte typical of Cyrillic in Windows-1251 (not UTF-8 mojibake pairs).
fn cp1251_cyrillic_byte(b: u8) -> bool {
    b >= 0xC0 || b == 0xA8 || b == 0xB8
}

fn likely_cp1251_latin1_mojibake(bytes: &[u8]) -> bool {
    let high = bytes.iter().filter(|&&b| cp1251_cyrillic_byte(b)).count();
    if high < 3 {
        return false;
    }
    let ascii_lower = bytes.iter().filter(|b| b.is_ascii_lowercase()).count();
    // Real Western Latin-1 titles (Été, ÆØÅ) stay short; CP1251-in-Latin1 tags mix many 0xC0–0xFF bytes
    // with faux-Latin lowercase (ì, ã, …) or are long runs like Ýêñêëþçèâ.
    high >= 5 || (high >= 4 && ascii_lower >= 1) || (high >= 3 && ascii_lower >= 2)
}

fn latin1_bytes(text: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(text.len());
    for c in text.chars() {
        let code = u32::from(c);
        if code > 0xFF {
            return None;
        }
        bytes.push(code as u8);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::TextEncoding;
    use lofty::config::{ParseOptions, WriteOptions};
    use lofty::file::AudioFile;
    use lofty::id3::v2::{Frame, FrameId, TextInformationFrame};
    use lofty::mpeg::MpegFile;
    use std::borrow::Cow;
    use std::fs::File;
    use std::io::BufReader;
    use std::path::Path;

    fn write_text_frame(path: &Path, id: &str, encoding: TextEncoding, value: &str) {
        let mut file = MpegFile::read_from(
            &mut BufReader::new(File::open(path).unwrap()),
            ParseOptions::new(),
        )
        .unwrap();
        let mut tag = file.id3v2_mut().cloned().unwrap_or_default();
        let frame_id = FrameId::Valid(Cow::Owned(id.to_string()));
        for _ in tag.remove(&frame_id) {}
        tag.insert(Frame::Text(TextInformationFrame::new(
            frame_id,
            encoding,
            value.to_string(),
        )));
        file.set_id3v2(tag);
        file.save_to_path(path, WriteOptions::default()).unwrap();
    }

    #[test]
    fn leaves_utf8_and_utf16_id3_alone() {
        for (encoding, title) in [
            (TextEncoding::UTF8, "Été"),
            (TextEncoding::UTF8, "ÉÉ"),
            (TextEncoding::UTF8, "ÆØÅ"),
            (TextEncoding::UTF8, "混合"),
        ] {
            assert_eq!(
                repair_latin1_id3_text(title, false),
                title,
                "{encoding:?} {title}"
            );
        }
        assert_eq!(repair_latin1_id3_text("Été", false), "Été");
    }

    #[test]
    fn leaves_western_latin1_accents_alone() {
        for name in [
            "Sigur Rós",
            "Mötley Crüe",
            "Björk",
            "Zoë",
            "François",
            "Étienne Daho",
            "Niño",
            "Édith Piaf",
            "Café",
            "Polar Night",
            "Été",
            "ÉÉ",
            "ÆØÅ",
        ] {
            assert_eq!(repair_latin1_id3_text(name, true), name, "{name}");
        }
    }

    #[test]
    fn repairs_cp1251_payload_marked_latin1() {
        assert_eq!(repair_latin1_id3_text("Ýêñêëþçèâ", true), "Эксклюзив");
        assert_eq!(repair_latin1_id3_text("09.Íîããàíî", true), "09.Ноггано");
    }

    #[test]
    fn read_track_keeps_utf8_id3_and_repairs_latin1_cp1251() {
        use crate::read_track;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("frames.mp3");
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tagged.mp3");
        std::fs::copy(&base, &path).unwrap();

        write_text_frame(&path, "TIT2", TextEncoding::UTF8, "Été");
        write_text_frame(&path, "TALB", TextEncoding::UTF8, "ÉÉ");
        let info = read_track(&path).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("Été"));
        assert_eq!(info.metadata.album.as_deref(), Some("ÉÉ"));

        write_text_frame(&path, "TPE1", TextEncoding::Latin1, "09.Íîããàíî");
        write_text_frame(&path, "TALB", TextEncoding::Latin1, "Ýêñêëþçèâ");
        let info = read_track(&path).unwrap();
        assert_eq!(info.metadata.artist.as_deref(), Some("09.Ноггано"));
        assert_eq!(info.metadata.album.as_deref(), Some("Эксклюзив"));

        write_text_frame(&path, "TIT2", TextEncoding::UTF16, "ÆØÅ");
        let info = read_track(&path).unwrap();
        assert_eq!(info.metadata.title.as_deref(), Some("ÆØÅ"));
    }
}
