//! Writing LRC: what the editor saves and what goes into tags and `.lrc` files.

use crate::Lyrics;
use crate::time::format_timestamp;

pub fn write(lyrics: &Lyrics) -> String {
    let mut out = String::new();
    for (key, value) in &lyrics.tags {
        out.push_str(&format!("[{key}:{value}]\n"));
    }
    if lyrics.offset_ms != 0 {
        out.push_str(&format!("[offset:{:+}]\n", lyrics.offset_ms));
    }
    for line in &lyrics.lines {
        let Some(start) = line.start.filter(|_| lyrics.synced) else {
            // Plain lyrics, or a line the editor hasn't timed yet.
            out.push_str(&line.text);
            out.push('\n');
            continue;
        };
        let stamp = format!("[{}]", format_timestamp(start));
        out.push_str(&stamp);
        if line.words.is_empty() {
            out.push_str(&line.text);
        } else {
            for word in &line.words {
                out.push_str(&format!("<{}>{}", format_timestamp(word.start), word.text));
            }
        }
        out.push('\n');
        if let Some(translation) = &line.translation {
            // The convention the parser reads back: the same timestamp again.
            out.push_str(&stamp);
            out.push_str(translation);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{Line, Lyrics, Word, parse};

    #[test]
    fn writes_tags_offset_lines_words_and_translations() {
        let lyrics = Lyrics {
            synced: true,
            offset_ms: -120,
            tags: vec![("ar".into(), "Artist".into())],
            lines: vec![
                Line {
                    start: Some(1_000),
                    text: "Hello world".into(),
                    words: vec![
                        Word {
                            start: 1_000,
                            text: "Hello ".into(),
                        },
                        Word {
                            start: 1_500,
                            text: "world".into(),
                        },
                    ],
                    translation: Some("Привет, мир".into()),
                },
                Line {
                    start: Some(4_000),
                    ..Line::default()
                },
            ],
        };
        let text = lyrics.to_lrc();
        assert_eq!(
            text,
            "[ar:Artist]\n[offset:-120]\n[00:01.00]<00:01.00>Hello <00:01.50>world\n\
             [00:01.00]Привет, мир\n[00:04.00]\n"
        );
        assert_eq!(parse(&text), lyrics);
    }

    #[test]
    fn writing_and_reading_again_changes_nothing() {
        for source in [
            "",
            "Just\nplain\n\ntext",
            "[ar:A]\n[offset:+300]\n[00:01.00]a\n[00:02.00]b",
            "[00:30.00][00:10.00]repeat\n[00:20.00]<00:20.00>w1 <00:20.50>w2",
            "[00:05.00]Line\n[00:05.00]Translation\n[00:09.999]rounded",
            "[00:01.00]Oh, [00:01.80]yeah\n[00:03.00]",
        ] {
            // Timestamps are written in hundredths (what every player reads), so the first
            // write may round; after that nothing changes.
            let once = parse(&parse(source).to_lrc());
            assert_eq!(parse(&once.to_lrc()), once, "{source:?}");
        }
    }

    #[test]
    fn plain_lyrics_stay_plain() {
        let lyrics = Lyrics::plain("One\n\nTwo");
        assert_eq!(lyrics.to_lrc(), "One\n\nTwo\n");
        assert_eq!(parse(&lyrics.to_lrc()), lyrics);
    }

    #[test]
    fn round_trips_real_world_lrc() {
        let source = "[ti:Song]\n[00:00.00]\n[00:12.34]First\n[00:12.34]Перевод\n\
                      [00:15.00]<00:15.00>Sec<00:15.40>ond <00:16.00>line\n[01:02.50]End\n";
        assert_eq!(parse(source).to_lrc(), source);
    }
}
