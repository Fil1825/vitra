//! ID3v2 `SYLT` frames: timed text entries. Taggers write either one entry per line, or one
//! per word or syllable with a line break starting (or ending) each line.

use crate::{Line, Lyrics, Word};

fn is_break(c: char) -> bool {
    c == '\n' || c == '\r'
}

/// Lyrics from `SYLT` entries (milliseconds, text).
pub fn from_sylt(entries: &[(u32, String)]) -> Lyrics {
    let by_word = entries
        .iter()
        .any(|(_, text)| text.starts_with(is_break) || text.ends_with(is_break));
    let mut lines: Vec<Line> = Vec::new();
    let mut new_line = true;
    for (time, text) in entries {
        let start = u64::from(*time);
        if !by_word || text.starts_with(is_break) {
            new_line = true;
        }
        let word = text.trim_matches(is_break).to_owned();
        if new_line || lines.is_empty() {
            lines.push(Line {
                start: Some(start),
                ..Line::default()
            });
        }
        if let Some(line) = lines.last_mut() {
            line.words.push(Word { start, text: word });
        }
        new_line = text.ends_with(is_break);
    }
    for line in &mut lines {
        line.words.retain(|word| !word.text.is_empty());
        line.text = line
            .words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<String>()
            .trim()
            .to_owned();
        // One word is just the line.
        if line.words.len() < 2 {
            line.words.clear();
        }
    }
    lines.sort_by_key(|line| line.start);
    Lyrics {
        lines,
        synced: true,
        ..Lyrics::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(list: &[(u32, &str)]) -> Vec<(u32, String)> {
        list.iter().map(|(t, s)| (*t, (*s).to_owned())).collect()
    }

    #[test]
    fn one_entry_per_line() {
        let lyrics = from_sylt(&entries(&[(1_000, "First line"), (4_000, "Second")]));
        assert!(lyrics.synced);
        let lines: Vec<_> = lyrics
            .lines
            .iter()
            .map(|l| (l.start, l.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [(Some(1_000), "First line"), (Some(4_000), "Second")]
        );
        assert!(lyrics.lines[0].words.is_empty());
    }

    #[test]
    fn syllables_with_line_breaks() {
        let lyrics = from_sylt(&entries(&[
            (1_000, "\nHel"),
            (1_200, "lo "),
            (1_500, "world"),
            (3_000, "\rNext"),
            (3_400, " line\n"),
            (5_000, "Last"),
        ]));
        let line = &lyrics.lines[0];
        assert_eq!(line.text, "Hello world");
        assert_eq!(line.words.len(), 3);
        assert_eq!(lyrics.lines[1].text, "Next line");
        assert_eq!(lyrics.lines[2].text, "Last");
        assert_eq!(lyrics.lines[2].start, Some(5_000));
    }

    #[test]
    fn empty_frames_are_empty() {
        assert!(from_sylt(&[]).is_empty());
    }
}
