//! Lyrics files. A track's `.lrc` sits next to it with the same name. They are mostly UTF-8,
//! but older ones come in UTF-16 (with a byte order mark) or in a legacy 8-bit code page:
//! Windows-1251 for Cyrillic ones, Latin-1 / Windows-1252 for Western ones.

use std::path::{Path, PathBuf};

/// Windows-1251 from 0x80 to 0xBF; 0xC0–0xFF are А–я in order.
const CP1251_HIGH: [char; 64] = [
    'Ђ', 'Ѓ', '‚', 'ѓ', '„', '…', '†', '‡', //
    '€', '‰', 'Љ', '‹', 'Њ', 'Ќ', 'Ћ', 'Џ', //
    'ђ', '‘', '’', '“', '”', '•', '–', '—', //
    '\u{fffd}', '™', 'љ', '›', 'њ', 'ќ', 'ћ', 'џ', //
    '\u{a0}', 'Ў', 'ў', 'Ј', '¤', 'Ґ', '¦', '§', //
    'Ё', '©', 'Є', '«', '¬', '\u{ad}', '®', 'Ї', //
    '°', '±', 'І', 'і', 'ґ', 'µ', '¶', '·', //
    'ё', '№', 'є', '»', 'ј', 'Ѕ', 'ѕ', 'ї', //
];

fn cp1251(byte: u8) -> char {
    match byte {
        0..=0x7f => char::from(byte),
        0x80..=0xbf => CP1251_HIGH[usize::from(byte - 0x80)],
        _ => char::from_u32(0x0410 + u32::from(byte - 0xc0)).unwrap_or('\u{fffd}'),
    }
}

fn utf16(bytes: &[u8], little_endian: bool) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|&pair| {
            if little_endian {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            }
        })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Text from a lyrics file in whatever encoding it uses.
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(b"\xef\xbb\xbf") {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(b"\xff\xfe") {
        return utf16(rest, true);
    }
    if let Some(rest) = bytes.strip_prefix(b"\xfe\xff") {
        return utf16(rest, false);
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    // In Cyrillic text nearly every letter is a high byte (А–я, Ё, ё); in Western text only
    // the accented ones are.
    let cyrillic = |b: u8| b >= 0xc0 || b == 0xa8 || b == 0xb8;
    let high = bytes.iter().filter(|b| cyrillic(**b)).count();
    let letters = high + bytes.iter().filter(|b| b.is_ascii_alphabetic()).count();
    if high * 2 > letters {
        bytes.iter().map(|&b| cp1251(b)).collect()
    } else {
        // Latin-1; Windows-1252 differs only in rarely used punctuation.
        bytes.iter().map(|&b| char::from(b)).collect()
    }
}

/// The `.lrc` file that goes with a track.
pub fn lrc_path(track: &Path) -> PathBuf {
    track.with_extension("lrc")
}

/// The text of a track's `.lrc` (`.LRC` too, on case-sensitive file systems), if it has one.
pub fn read_lrc(track: &Path) -> Option<String> {
    [lrc_path(track), track.with_extension("LRC")]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
        .map(|bytes| decode_text(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_lrc_next_to_a_track() {
        let dir = std::env::temp_dir().join(format!("vitra-lrc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let track = dir.join("01 Song.flac");
        assert_eq!(read_lrc(&track), None);
        std::fs::write(dir.join("01 Song.lrc"), "[00:01.00]Hi").unwrap();
        assert_eq!(read_lrc(&track).as_deref(), Some("[00:01.00]Hi"));
        assert_eq!(lrc_path(&track), dir.join("01 Song.lrc"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_utf8_with_and_without_a_bom() {
        assert_eq!(decode_text("[00:01.00]Ёлка".as_bytes()), "[00:01.00]Ёлка");
        assert_eq!(decode_text(b"\xef\xbb\xbf[ar:A]"), "[ar:A]");
    }

    #[test]
    fn reads_utf16() {
        let text = "[00:01.00]Привет";
        let le: Vec<u8> = [0xff, 0xfe]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        let be: Vec<u8> = [0xfe, 0xff]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
            .collect();
        assert_eq!(decode_text(&le), text);
        assert_eq!(decode_text(&be), text);
    }

    #[test]
    fn guesses_legacy_code_pages() {
        // "[00:01.00]Ёлки-палки, ёж «№1»" in Windows-1251.
        let cyrillic = b"[00:01.00]\xa8\xeb\xea\xe8-\xef\xe0\xeb\xea\xe8, \xb8\xe6 \xab\xb91\xbb";
        assert_eq!(decode_text(cyrillic), "[00:01.00]Ёлки-палки, ёж «№1»");
        // "Café de l'été" in Latin-1.
        assert_eq!(decode_text(b"Caf\xe9 de l'\xe9t\xe9"), "Café de l'été");
        // Every Cyrillic letter maps.
        let alphabet: Vec<u8> = (0xc0..=0xff).collect();
        let decoded = decode_text(&alphabet);
        assert!(decoded.starts_with("АБВ") && decoded.ends_with("эюя"));
    }
}
