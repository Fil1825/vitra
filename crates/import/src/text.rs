//! Pasted text: one track per line ("Artist — Title", "Title – Artist", a track link), a
//! table copied from a spreadsheet or the Music app, or blocks copied from a web page (a
//! title, an artist and a length, over and over, as VK and Yandex Music lay them out).

use crate::model::{ImportedTrack, is_duration, parse_duration};
use crate::table;

/// One line or block of pasted text.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Track(ImportedTrack),
    /// A link to a track or a list on a service, read later.
    Link(String),
}

/// What pasted text holds, in order.
pub fn parse(text: &str) -> Vec<Item> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }
    // A table with a header row (tabs or commas).
    if let Some(tracks) = table::parse(&text) {
        return tracks.into_iter().map(Item::Track).collect();
    }
    let durations = lines.iter().filter(|line| is_duration(line)).count();
    if durations >= 2 && durations * 2 >= lines.len() / 3 {
        return blocks(&lines).into_iter().map(Item::Track).collect();
    }
    lines.iter().filter_map(|line| line_item(line)).collect()
}

/// A link on its own, if the line is one.
fn link(line: &str) -> Option<String> {
    let first = line.split_whitespace().next()?;
    let lower = first.to_ascii_lowercase();
    let is_link = lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("spotify:");
    (is_link && first.len() == line.len()).then(|| first.to_owned())
}

/// "1. Artist - Title (3:45)" and the like.
fn line_item(line: &str) -> Option<Item> {
    if let Some(url) = link(line) {
        return Some(Item::Link(url));
    }
    let mut rest = strip_number(line);
    let mut duration = None;
    // A length at the end: "… 3:45", "… (3:45)", "…\t3:45".
    if let Some((head, tail)) = rest.rsplit_once([' ', '\t']) {
        let tail = tail.trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']');
        if is_duration(tail) {
            duration = parse_duration(tail);
            rest = head.trim_end();
        }
    }
    let rest = rest.trim();
    if rest.is_empty() || is_noise(rest) {
        return None;
    }
    let mut track = split_credit(rest);
    track.duration = duration;
    Some(Item::Track(track))
}

/// "Artist - Title": which side is the artist isn't known, so matching tries both.
fn split_credit(text: &str) -> ImportedTrack {
    for separator in ["\t", " — ", " – ", " - ", " -- ", " | "] {
        if let Some((left, right)) = text.split_once(separator) {
            let (left, right) = (left.trim(), right.trim());
            if !left.is_empty() && !right.is_empty() {
                return ImportedTrack {
                    swappable: true,
                    ..ImportedTrack::new(right, vec![left.to_owned()])
                };
            }
        }
    }
    ImportedTrack::new(text, Vec::new())
}

/// Without a leading track number ("1.", "01 ", "12)").
fn strip_number(line: &str) -> &str {
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 4 {
        return line;
    }
    let rest = &line[digits..];
    let stripped = rest
        .strip_prefix(". ")
        .or_else(|| rest.strip_prefix(") "))
        .or_else(|| rest.strip_prefix(".\t"))
        .or_else(|| rest.strip_prefix('\t'))
        .or_else(|| rest.strip_prefix(' '));
    stripped.map_or(line, str::trim_start)
}

/// Lines a page puts between the names: positions, badges, dates, buttons.
fn is_noise(line: &str) -> bool {
    let lower = line.to_lowercase();
    if line
        .chars()
        .all(|c| c.is_ascii_digit() || c.is_whitespace())
    {
        return true;
    }
    const WORDS: &[&str] = &[
        "e",
        "explicit",
        "•",
        "·",
        "lyrics",
        "текст",
        "video",
        "видео",
        "song",
        "песня",
        "play",
        "слушать",
        "сейчас играет",
        "now playing",
        "добавить",
        "add",
        "…",
        "...",
    ];
    if WORDS.contains(&lower.as_str()) {
        return true;
    }
    // "3 days ago", "2 недели назад", "2024-01-05", "5 янв. 2024 г.".
    let has_digit = lower.chars().any(|c| c.is_ascii_digit());
    has_digit
        && (lower.ends_with(" ago")
            || lower.ends_with(" назад")
            || lower.ends_with(" г.")
            || is_date(&lower)
            || lower.split_whitespace().count() <= 4
                && MONTHS.iter().any(|month| lower.contains(month)))
}

const MONTHS: &[&str] = &[
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec", "янв",
    "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
];

fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split(['-', '.', '/']).collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Blocks ending in a length: the first lines that aren't noise are the title, the artist
/// and the album, in the order most pages show them (title first). Some pages put the
/// artist first, so the two may be swapped in matching.
fn blocks(lines: &[&str]) -> Vec<ImportedTrack> {
    let mut tracks = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    for line in lines {
        if is_duration(line) {
            let names: Vec<&str> = block
                .iter()
                .copied()
                .map(strip_number)
                .filter(|l| !l.is_empty() && !is_noise(l))
                .collect();
            let mut track = match names.as_slice() {
                [] => {
                    block.clear();
                    continue;
                }
                [one] => split_credit(one),
                [title, artist, rest @ ..] => ImportedTrack {
                    album: rest.first().map(|album| (*album).to_owned()),
                    swappable: true,
                    ..ImportedTrack::new(*title, vec![(*artist).to_owned()])
                },
            };
            track.duration = parse_duration(line);
            tracks.push(track);
            block.clear();
        } else {
            block.push(line);
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracks(items: Vec<Item>) -> Vec<ImportedTrack> {
        items
            .into_iter()
            .filter_map(|item| match item {
                Item::Track(track) => Some(track),
                Item::Link(_) => None,
            })
            .collect()
    }

    #[test]
    fn one_track_per_line() {
        let items = parse(
            "1. Daft Punk - One More Time\n\
             02 Кино — Группа крови (4:45)\n\
             Numb\n\
             \n\
             Massive Attack – Teardrop\t5:29\n",
        );
        let found = tracks(items);
        assert_eq!(found.len(), 4);
        assert_eq!(found[0].title, "One More Time");
        assert_eq!(found[0].artists, ["Daft Punk"]);
        assert!(found[0].swappable);
        assert_eq!(found[1].title, "Группа крови");
        assert_eq!(found[1].duration, Some(285.0));
        assert_eq!(found[2].title, "Numb");
        assert!(found[2].artists.is_empty());
        assert_eq!(found[3].duration, Some(329.0));
    }

    #[test]
    fn links_stay_links() {
        let items = parse(
            "https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC\n\
             spotify:track:7ouMYWpwJ422jRcDASZB7P\n",
        );
        assert_eq!(
            items,
            [
                Item::Link("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC".into()),
                Item::Link("spotify:track:7ouMYWpwJ422jRcDASZB7P".into()),
            ]
        );
    }

    #[test]
    fn blocks_copied_from_a_page() {
        // VK: title, artist, length.
        let vk = "Группа крови\nКино\n4:45\nЗвезда по имени Солнце\nКино\nE\n3:45\n";
        let found = tracks(parse(vk));
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].title, "Группа крови");
        assert_eq!(found[0].artists, ["Кино"]);
        assert_eq!(found[1].duration, Some(225.0));
        // Spotify's web player: number, title, artist, album, date added, length.
        let spotify = "1\nOne More Time\nDaft Punk\nDiscovery\n3 days ago\n5:20\n\
                       2\nTeardrop\nMassive Attack\nMezzanine\nJan 5, 2024\n5:29\n";
        let found = tracks(parse(spotify));
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].title, "Teardrop");
        assert_eq!(found[1].artists, ["Massive Attack"]);
        assert_eq!(found[1].album.as_deref(), Some("Mezzanine"));
    }

    #[test]
    fn tables_with_headers() {
        let sheet = "Title\tArtist\tAlbum\tTime\nTeardrop\tMassive Attack\tMezzanine\t5:29\n";
        let found = tracks(parse(sheet));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].artists, ["Massive Attack"]);
        assert_eq!(found[0].duration, Some(329.0));
        assert!(!found[0].swappable);
    }
}
