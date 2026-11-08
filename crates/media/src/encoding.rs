//! Legacy tag text: ID3v2 often marks Cyrillic as ISO-8859-1 (encoding byte `0`) while the payload
//! is Windows-1251. Lofty decodes those bytes as Latin-1, which yields mojibake.

use lofty::tag::TagType;

use lyrics::decode_text;

/// Bump when tag decoding logic changes so the library scanner re-reads affected tracks.
pub const TAG_DECODER_VERSION: i64 = 1;

/// Corrects likely Windows-1251-in-ISO-8859-1 mis-decoding for legacy **ID3** tags only.
pub fn repair_id3_tag_text(text: &str, tag_type: TagType) -> String {
    if text.is_empty() || !is_legacy_id3(tag_type) || text.chars().any(is_cyrillic) {
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

fn is_legacy_id3(tag_type: TagType) -> bool {
    matches!(tag_type, TagType::Id3v2 | TagType::Id3v1)
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
    if high < 2 {
        return false;
    }
    let letters = high + bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    high * 2 > letters
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

    #[test]
    fn leaves_valid_utf8_cyrillic_alone() {
        assert_eq!(repair_id3_tag_text("Ноггано", TagType::Id3v2), "Ноггано");
    }

    #[test]
    fn leaves_western_names_with_unicode_accents_alone() {
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
        ] {
            assert_eq!(repair_id3_tag_text(name, TagType::Id3v2), name, "{name}");
        }
    }

    #[test]
    fn skips_non_id3_formats_even_when_bytes_look_cyrillic() {
        let mojibake = "Ýêñêëþçèâ";
        assert_eq!(
            repair_id3_tag_text(mojibake, TagType::VorbisComments),
            mojibake
        );
        assert_eq!(repair_id3_tag_text(mojibake, TagType::Mp4Ilst), mojibake);
    }

    #[test]
    fn repairs_cp1251_payload_marked_latin1_in_id3() {
        assert_eq!(
            repair_id3_tag_text("Ýêñêëþçèâ", TagType::Id3v2),
            "Эксклюзив"
        );
        assert_eq!(
            repair_id3_tag_text("09.Íîããàíî", TagType::Id3v2),
            "09.Ноггано"
        );
    }
}
