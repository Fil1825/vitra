//! Reading LRC, enhanced LRC and plain lyrics.

use crate::time::parse_timestamp;
use crate::{Line, Lyrics, Word};

/// `[key:value]` metadata such as `[ar:Artist]` or `[offset:+250]`.
fn id_tag(inner: &str) -> Option<(String, String)> {
    let (key, value) = inner.split_once(':')?;
    let key = key.trim();
    let valid = !key.is_empty()
        && key.len() <= 16
        && key.chars().all(|c| c.is_ascii_alphabetic() || c == '#');
    valid.then(|| (key.to_ascii_lowercase(), value.trim().to_owned()))
}

/// Splits `[...]` groups off the start of a line: its timestamps, or one metadata tag.
fn leading_tags(line: &str) -> (Vec<u64>, Option<(String, String)>, &str) {
    let mut stamps = Vec::new();
    let mut rest = line;
    while let Some(after) = rest.strip_prefix('[') {
        let Some(end) = after.find(']') else {
            break;
        };
        let inner = &after[..end];
        if let Some(time) = parse_timestamp(inner) {
            stamps.push(time);
        } else if stamps.is_empty()
            && let Some(tag) = id_tag(inner)
        {
            return (stamps, Some(tag), after[end + 1..].trim());
        } else {
            // Text that happens to start with brackets, like "[Chorus]".
            break;
        }
        rest = after[end + 1..].trim_start();
    }
    (stamps, None, rest)
}

/// The next word timestamp in `text` (`<mm:ss.xx>`, or `[mm:ss.xx]` inside a line): its byte
/// range and time.
fn next_word_stamp(text: &str) -> Option<(usize, usize, u64)> {
    let mut from = 0;
    while let Some(offset) = text[from..].find(['<', '[']) {
        let open = from + offset;
        let close_char = if text[open..].starts_with('<') {
            '>'
        } else {
            ']'
        };
        if let Some(len) = text[open + 1..].find(close_char)
            && let Some(time) = parse_timestamp(&text[open + 1..open + 1 + len])
        {
            return Some((open, open + 1 + len + 1, time));
        }
        from = open + 1;
    }
    None
}

/// A line's text and its word timings, if it has any. Text before the first word timestamp
/// starts with the line.
fn words(text: &str, line_start: u64) -> (String, Vec<Word>) {
    let mut words = Vec::new();
    let mut rest = text;
    let mut start = line_start;
    while let Some((open, close, time)) = next_word_stamp(rest) {
        if !rest[..open].is_empty() {
            words.push(Word {
                start,
                text: rest[..open].to_owned(),
            });
        }
        start = time;
        rest = &rest[close..];
    }
    if words.is_empty() && rest.len() == text.len() {
        return (text.trim().to_owned(), Vec::new());
    }
    if !rest.is_empty() {
        words.push(Word {
            start,
            text: rest.to_owned(),
        });
    }
    // Leading and trailing spaces belong to no word.
    if let Some(first) = words.first_mut() {
        first.text = first.text.trim_start().to_owned();
    }
    if let Some(last) = words.last_mut() {
        last.text = last.text.trim_end().to_owned();
    }
    words.retain(|word| !word.text.is_empty());
    let line: String = words.iter().map(|word| word.text.as_str()).collect();
    (line, words)
}

/// Parses lyrics: LRC when any line has a timestamp, plain text otherwise. Never fails;
/// anything unreadable is kept as text.
pub fn parse(input: &str) -> Lyrics {
    let input = input.trim_start_matches('\u{feff}');
    let mut lyrics = Lyrics::default();
    // (start, file order, line): sorting by both keeps lines with equal times in file order.
    let mut timed: Vec<(u64, usize, Line)> = Vec::new();
    let mut untimed: Vec<String> = Vec::new();

    for (order, raw) in input.lines().enumerate() {
        let (stamps, tag, rest) = leading_tags(raw.trim());
        if let Some((key, value)) = tag {
            if key == "offset" {
                lyrics.offset_ms = value.trim_start_matches('+').parse().unwrap_or(0);
            } else {
                lyrics.tags.push((key, value));
            }
            continue;
        }
        let Some(&first) = stamps.first() else {
            untimed.push(raw.trim_end().to_owned());
            continue;
        };
        let (text, line_words) = words(rest, first);
        for &start in &stamps {
            // A line sung at several times keeps its word timings relative to each.
            let shifted = line_words
                .iter()
                .map(|word| Word {
                    start: (word.start + start).saturating_sub(first),
                    text: word.text.clone(),
                })
                .collect();
            timed.push((
                start,
                order,
                Line {
                    start: Some(start),
                    text: text.clone(),
                    words: shifted,
                    translation: None,
                },
            ));
        }
    }

    if timed.is_empty() {
        let first = untimed.iter().position(|l| !l.trim().is_empty());
        let last = untimed.iter().rposition(|l| !l.trim().is_empty());
        if let (Some(first), Some(last)) = (first, last) {
            lyrics.lines = untimed[first..=last]
                .iter()
                .map(|text| Line {
                    text: text.clone(),
                    ..Line::default()
                })
                .collect();
        }
        return lyrics;
    }

    // Untimed lines in an LRC file are headers or credits, not lyrics.
    lyrics.synced = true;
    timed.sort_by_key(|(start, order, _)| (*start, *order));
    for (_, _, line) in timed {
        match lyrics.lines.last_mut() {
            Some(previous)
                if previous.start == line.start
                    && previous.translation.is_none()
                    && !previous.text.is_empty()
                    && !line.text.is_empty() =>
            {
                previous.translation = Some(line.text);
            }
            _ => lyrics.lines.push(line),
        }
    }
    lyrics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lyrics: &Lyrics) -> Vec<(Option<u64>, &str)> {
        lyrics
            .lines
            .iter()
            .map(|l| (l.start, l.text.as_str()))
            .collect()
    }

    #[test]
    fn reads_plain_lyrics() {
        let lyrics = parse("\n\nFirst line\n\nSecond verse  \r\n[Chorus]\n\n");
        assert!(!lyrics.synced);
        assert_eq!(
            texts(&lyrics),
            [
                (None, "First line"),
                (None, ""),
                (None, "Second verse"),
                (None, "[Chorus]")
            ]
        );
    }

    #[test]
    fn reads_lrc_with_tags_and_offset() {
        let lyrics = parse(
            "\u{feff}[ti:Song]\r\n[ar:Кино]\n[offset:+250]\n[length: 03:20]\n\
             [00:12.00]Первая строка\n[00:15.50] Second line \n[00:20.00]\n[00:21.00]Last",
        );
        assert!(lyrics.synced);
        assert_eq!(lyrics.offset_ms, 250);
        assert_eq!(
            lyrics.tags,
            [
                ("ti".to_owned(), "Song".to_owned()),
                ("ar".to_owned(), "Кино".to_owned()),
                ("length".to_owned(), "03:20".to_owned()),
            ]
        );
        assert_eq!(
            texts(&lyrics),
            [
                (Some(12_000), "Первая строка"),
                (Some(15_500), "Second line"),
                (Some(20_000), ""),
                (Some(21_000), "Last"),
            ]
        );
        assert!(lyrics.lines[2].is_gap());
        assert_eq!(parse("[offset:-100]\n[00:01.00]a").offset_ms, -100);
    }

    #[test]
    fn repeats_lines_with_several_timestamps_in_time_order() {
        let lyrics = parse("[00:30.00][00:10.00]Chorus\n[00:20.00]Verse");
        assert_eq!(
            texts(&lyrics),
            [
                (Some(10_000), "Chorus"),
                (Some(20_000), "Verse"),
                (Some(30_000), "Chorus")
            ]
        );
    }

    #[test]
    fn reads_word_timings() {
        let lyrics = parse("[00:01.00]<00:01.00>Hello <00:01.50>big <00:02.00>world<00:03.00>");
        let line = &lyrics.lines[0];
        assert_eq!(line.text, "Hello big world");
        let words: Vec<(u64, &str)> = line
            .words
            .iter()
            .map(|w| (w.start, w.text.as_str()))
            .collect();
        assert_eq!(
            words,
            [(1_000, "Hello "), (1_500, "big "), (2_000, "world")]
        );

        // Text before the first word timestamp starts with the line; square brackets work too.
        let line = &parse("[00:05.00]Oh, [00:05.80]yeah")[0];
        let words: Vec<(u64, &str)> = line
            .words
            .iter()
            .map(|w| (w.start, w.text.as_str()))
            .collect();
        assert_eq!(words, [(5_000, "Oh, "), (5_800, "yeah")]);
    }

    #[test]
    fn repeated_word_lines_move_their_words() {
        let lyrics = parse("[00:01.00][01:01.00]<00:01.00>la <00:01.50>la");
        assert_eq!(lyrics.lines[1].words[1].start, 61_500);
    }

    #[test]
    fn a_repeated_timestamp_is_a_translation() {
        let lyrics = parse(
            "[00:10.00]Я люблю тебя\n[00:10.00]I love you\n[00:14.00]Ещё\n[00:14.00]More\n\
             [00:14.00]Third with the same time",
        );
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].translation.as_deref(), Some("I love you"));
        assert_eq!(lyrics.lines[1].translation.as_deref(), Some("More"));
        // Only one translation per line; a third line stays a line.
        assert_eq!(lyrics.lines[2].text, "Third with the same time");
    }

    #[test]
    fn untimed_lines_in_lrc_are_dropped_and_brackets_stay_text() {
        let lyrics = parse("Lyrics by someone\n[00:01.00][Chorus] la\n[not a tag]\n[00:02.00]b");
        assert_eq!(
            texts(&lyrics),
            [(Some(1_000), "[Chorus] la"), (Some(2_000), "b")]
        );
        let plain = parse("[not a tag]\n<00:01.00> is text here");
        assert!(!plain.synced);
        assert_eq!(plain.lines[0].text, "[not a tag]");
    }

    #[test]
    fn empty_input_is_empty_lyrics() {
        assert!(parse("").is_empty());
        assert!(parse("  \n\n").lines.is_empty());
        assert!(parse("[ar:Only tags]").is_empty());
    }

    impl std::ops::Index<usize> for Lyrics {
        type Output = Line;
        fn index(&self, index: usize) -> &Line {
            &self.lines[index]
        }
    }
}
