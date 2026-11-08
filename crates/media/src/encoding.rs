//! Legacy tag text: ID3v1/v2 often marks Cyrillic as ISO-8859-1 (encoding byte `0`) while the
//! payload is actually Windows-1251. Lofty decodes those bytes as Latin-1, which yields
//! mojibake such as `Ýëñêëþçèâ` instead of «Эксклюзив».

use lyrics::decode_text;

/// Windows-1251 Cyrillic in the string's Latin-1 code points, if that reading is likely.
pub fn repair_legacy_cyrillic(text: &str) -> String {
    if text.is_empty() || text.chars().any(is_cyrillic) {
        return text.to_owned();
    }
    let Some(bytes) = latin1_bytes(text) else {
        return text.to_owned();
    };
    let repaired = decode_text(&bytes);
    if repaired != text && repaired.chars().any(is_cyrillic) {
        repaired
    } else {
        text.to_owned()
    }
}

fn is_cyrillic(c: char) -> bool {
    matches!(c, '\u{0400}'..='\u{04FF}' | '\u{0500}'..='\u{052F}')
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
        assert_eq!(repair_legacy_cyrillic("Ноггано"), "Ноггано");
        assert_eq!(repair_legacy_cyrillic("Эксклюзив"), "Эксклюзив");
    }

    #[test]
    fn leaves_ascii_and_western_latin1_alone() {
        assert_eq!(repair_legacy_cyrillic("Polar Night"), "Polar Night");
        assert_eq!(repair_legacy_cyrillic("Café de l'été"), "Café de l'été");
    }

    #[test]
    fn repairs_id3_latin1_mojibake_of_cp1251() {
        let mojibake = "Ýêñêëþçèâ".to_owned();
        assert_eq!(repair_legacy_cyrillic(&mojibake), "Эксклюзив");
        let artist = "09.Íîããàíî".to_owned();
        assert_eq!(repair_legacy_cyrillic(&artist), "09.Ноггано");
    }
}
