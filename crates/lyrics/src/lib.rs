//! Lyrics: one implementation of LRC and enhanced LRC (word timings) shared by tag reading,
//! `.lrc` files, search indexing, the lyrics view and the editor.
//!
//! - Plain text is lyrics without timing ([`Lyrics::synced`] is false).
//! - Each `[mm:ss.xx]` line is a [`Line`]; a line with several timestamps is repeated at each.
//! - `<mm:ss.xx>` inside a line (and `[mm:ss.xx]` after its text starts) time single [`Word`]s.
//! - A line repeating the previous line's timestamp is its **translation** (the convention of
//!   lyrics sites that ship both in one file).
//! - `[offset:±ms]` is applied by [`Lyrics::normalized`]; other `[key:value]` tags are kept.

mod parse;
mod sylt;
mod time;
mod write;

use serde::{Deserialize, Serialize};

pub use parse::parse;
pub use sylt::from_sylt;
pub use time::{format_timestamp, parse_timestamp};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    pub lines: Vec<Line>,
    /// The lines have timestamps; plain lyrics just scroll.
    pub synced: bool,
    /// `[offset:…]`: positive values make the lyrics appear sooner (LRC's convention).
    pub offset_ms: i64,
    /// Other `[key:value]` tags (`ti`, `ar`, `al`, `by`, `length`, ...), in file order.
    pub tags: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// Milliseconds; `None` in plain lyrics.
    pub start: Option<u64>,
    pub text: String,
    /// Word timings (enhanced LRC). Their texts, joined, are the line's text.
    #[serde(default)]
    pub words: Vec<Word>,
    #[serde(default)]
    pub translation: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub start: u64,
    /// The word with the spaces around it as written.
    pub text: String,
}

impl Lyrics {
    /// Plain lyrics from text, one line per line.
    pub fn plain(text: &str) -> Self {
        Self {
            lines: text
                .lines()
                .map(|line| Line {
                    text: line.trim_end().to_owned(),
                    ..Line::default()
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.iter().all(|line| line.text.trim().is_empty())
    }

    /// The words alone, one line per line (for search and for plain-text tags).
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for line in &self.lines {
            text.push_str(line.text.trim_end());
            text.push('\n');
        }
        text.trim_matches('\n').to_owned()
    }

    /// With the `[offset:…]` folded into the timestamps, so they read as positions in the track.
    pub fn normalized(&self) -> Self {
        let mut lyrics = self.clone();
        lyrics.shift(-self.offset_ms);
        lyrics.offset_ms = 0;
        lyrics
    }

    /// Moves every timestamp by `delta_ms` (later when positive), stopping at zero.
    pub fn shift(&mut self, delta_ms: i64) {
        let moved = |time: u64| time.saturating_add_signed(delta_ms);
        for line in &mut self.lines {
            line.start = line.start.map(moved);
            for word in &mut line.words {
                word.start = moved(word.start);
            }
        }
    }

    /// The line playing at `position_ms` in synced lyrics: the last one that has started.
    /// Expects [`Self::normalized`] timestamps.
    pub fn line_at(&self, position_ms: u64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let started = self
            .lines
            .partition_point(|line| line.start.is_some_and(|start| start <= position_ms));
        started.checked_sub(1)
    }

    /// LRC text (enhanced LRC where lines have word timings); plain lyrics as plain text.
    pub fn to_lrc(&self) -> String {
        write::write(self)
    }
}

impl Line {
    /// A synced line with no words: an instrumental gap.
    pub fn is_gap(&self) -> bool {
        self.start.is_some() && self.text.trim().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synced() -> Lyrics {
        parse("[offset:+500]\n[00:10.00]One\n[00:12.00]<00:12.00>Two <00:13.00>words\n[00:20.00]")
    }

    #[test]
    fn normalizing_applies_the_offset() {
        let lyrics = synced().normalized();
        assert_eq!(lyrics.offset_ms, 0);
        assert_eq!(lyrics.lines[0].start, Some(9_500));
        assert_eq!(lyrics.lines[1].words[1].start, 12_500);
        // A negative offset delays; nothing goes below zero.
        let early = parse("[offset:-1000]\n[00:00.50]x").normalized();
        assert_eq!(early.lines[0].start, Some(1_500));
        let mut shifted = early.clone();
        shifted.shift(-10_000);
        assert_eq!(shifted.lines[0].start, Some(0));
    }

    #[test]
    fn finds_the_line_playing() {
        let lyrics = synced().normalized();
        assert_eq!(lyrics.line_at(0), None);
        assert_eq!(lyrics.line_at(9_499), None);
        assert_eq!(lyrics.line_at(9_500), Some(0));
        // The offset moved the second line to 11.5 s too.
        assert_eq!(lyrics.line_at(11_499), Some(0));
        assert_eq!(lyrics.line_at(11_500), Some(1));
        assert_eq!(lyrics.line_at(1_000_000), Some(2));
        assert_eq!(Lyrics::plain("a\nb").line_at(5_000), None);
    }

    #[test]
    fn plain_text_drops_timing() {
        assert_eq!(synced().plain_text(), "One\nTwo words");
        assert_eq!(Lyrics::plain("a\n\nb\n").plain_text(), "a\n\nb");
        assert!(Lyrics::plain("  \n").is_empty());
    }

    #[test]
    fn serializes_camel_case_for_the_window() {
        let json = serde_json::to_value(parse("[00:01.00]Hi")).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "lines": [{ "start": 1000, "text": "Hi", "words": [], "translation": null }],
                "synced": true,
                "offsetMs": 0,
                "tags": []
            })
        );
    }
}
