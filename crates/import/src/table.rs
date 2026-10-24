//! Tables of tracks: CSV from exporters (Exportify, TuneMyMusic, Soundiiz, FreeYourMusic),
//! the Music app's text export (tab-separated) and rows copied from a spreadsheet. Columns
//! are found by their header's name, in English or Russian.

use crate::model::{ImportedTrack, parse_duration};

/// What a column holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Title,
    Artist,
    Album,
    /// Milliseconds when the header says so or the numbers are big, else seconds or "3:45".
    Duration,
    Isrc,
    Url,
    Path,
    Playlist,
}

/// Header names (lower case) for each column. The first match wins, so the specific names
/// come before the general ones ("track name" before "name").
const HEADERS: &[(&str, Column)] = &[
    ("track name", Column::Title),
    ("track title", Column::Title),
    ("song name", Column::Title),
    ("song title", Column::Title),
    ("title", Column::Title),
    ("name", Column::Title),
    ("song", Column::Title),
    ("track", Column::Title),
    ("название", Column::Title),
    ("трек", Column::Title),
    ("песня", Column::Title),
    ("artist name(s)", Column::Artist),
    ("artist names", Column::Artist),
    ("artist name", Column::Artist),
    ("artists", Column::Artist),
    ("artist", Column::Artist),
    ("исполнитель", Column::Artist),
    ("исполнители", Column::Artist),
    ("артист", Column::Artist),
    ("album name", Column::Album),
    ("album", Column::Album),
    ("release", Column::Album),
    ("альбом", Column::Album),
    ("duration (ms)", Column::Duration),
    ("duration_ms", Column::Duration),
    ("track duration (ms)", Column::Duration),
    ("duration", Column::Duration),
    ("time", Column::Duration),
    ("length", Column::Duration),
    ("длительность", Column::Duration),
    ("время", Column::Duration),
    ("isrc", Column::Isrc),
    ("track uri", Column::Url),
    ("spotify uri", Column::Url),
    ("url", Column::Url),
    ("link", Column::Url),
    ("location", Column::Path),
    ("playlist name", Column::Playlist),
    ("playlist", Column::Playlist),
];

fn column(header: &str) -> Option<Column> {
    let header = header
        .trim()
        .trim_matches('"')
        .trim_start_matches('\u{feff}')
        .to_lowercase();
    HEADERS
        .iter()
        .find(|(name, _)| header == *name)
        .map(|(_, column)| *column)
}

/// Rows of a CSV or tab-separated text, quotes understood (RFC 4180).
pub fn rows(text: &str, delimiter: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            quoted = true;
        } else if c == delimiter {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            if row.iter().any(|f| !f.trim().is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
        } else {
            field.push(c);
        }
    }
    row.push(field);
    if row.iter().any(|f| !f.trim().is_empty()) {
        rows.push(row);
    }
    rows
}

/// The delimiter the first line uses most: tab, comma or semicolon (European Excel).
fn delimiter(text: &str) -> char {
    let first = text.lines().next().unwrap_or_default();
    ['\t', ',', ';']
        .into_iter()
        .max_by_key(|d| first.matches(*d).count())
        .unwrap_or(',')
}

/// The tracks of a table whose first row names its columns; `None` when it has no title
/// column, so isn't such a table.
pub fn parse(text: &str) -> Option<Vec<ImportedTrack>> {
    parse_named(text).map(|(tracks, _)| tracks)
}

/// Like [`parse`], with the playlist's name when a column gives one.
pub fn parse_named(text: &str) -> Option<(Vec<ImportedTrack>, Option<String>)> {
    let delimiter = delimiter(text);
    let rows = rows(text, delimiter);
    let (header, body) = rows.split_first()?;
    let columns: Vec<Option<Column>> = header.iter().map(|h| column(h)).collect();
    // The first column of each kind counts ("Name" in the Music app is the title, a later
    // "Composer" isn't recognised anyway).
    let find = |wanted: Column| columns.iter().position(|c| *c == Some(wanted));
    let title = find(Column::Title)?;
    // A header with only a title column is probably not a header (a one-column list).
    if columns.iter().flatten().count() < 2 {
        return None;
    }
    let artist = find(Column::Artist);
    let album = find(Column::Album);
    let duration = find(Column::Duration);
    let millis = duration.is_some_and(|d| header[d].to_lowercase().contains("ms"));
    let isrc = find(Column::Isrc);
    let url = find(Column::Url);
    let path = find(Column::Path);
    let playlist = find(Column::Playlist);
    let cell = |row: &Vec<String>, index: Option<usize>| {
        index
            .and_then(|i| row.get(i))
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let mut name = None;
    let tracks = body
        .iter()
        .filter_map(|row| {
            let title = cell(row, Some(title))?;
            if name.is_none() {
                name = cell(row, playlist);
            }
            // Exportify lists several artists with commas; others give one credit.
            let artists = cell(row, artist)
                .map(|credit| vec![credit])
                .unwrap_or_default();
            let duration = cell(row, duration).and_then(|value| {
                let seconds = parse_duration(&value)?;
                // Values like 245000 are milliseconds even without "(ms)" in the header.
                Some(if millis || seconds > 36_000.0 {
                    seconds / 1000.0
                } else {
                    seconds
                })
            });
            Some(ImportedTrack {
                album: cell(row, album),
                duration,
                isrc: cell(row, isrc),
                url: cell(row, url).map(|u| spotify_uri_to_url(&u)),
                path: cell(row, path),
                ..ImportedTrack::new(title, artists)
            })
        })
        .collect();
    Some((tracks, name))
}

/// `spotify:track:ID` as the track's web link.
fn spotify_uri_to_url(uri: &str) -> String {
    match uri.strip_prefix("spotify:track:") {
        Some(id) => format!("https://open.spotify.com/track/{id}"),
        None => uri.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_quoted_csv() {
        let rows = rows("a,\"b, c\",\"say \"\"hi\"\"\"\n\n1,2,3\n", ',');
        assert_eq!(rows, [vec!["a", "b, c", "say \"hi\""], vec!["1", "2", "3"]]);
    }

    #[test]
    fn exportify() {
        let csv = "\"Track URI\",\"Track Name\",\"Artist URI(s)\",\"Artist Name(s)\",\"Album Name\",\"Duration (ms)\",\"ISRC\"\n\
                   \"spotify:track:4uLU6hMCjMI75M1A2tKUQC\",\"Never Gonna Give You Up\",\"spotify:artist:0gx\",\"Rick Astley\",\"Whenever You Need Somebody\",\"213573\",\"GBARL9300135\"\n";
        let tracks = parse(csv).unwrap();
        assert_eq!(tracks.len(), 1);
        let track = &tracks[0];
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.artists, ["Rick Astley"]);
        assert_eq!(track.album.as_deref(), Some("Whenever You Need Somebody"));
        assert!((track.duration.unwrap() - 213.573).abs() < 1e-6);
        assert_eq!(track.isrc.as_deref(), Some("GBARL9300135"));
        assert_eq!(
            track.url.as_deref(),
            Some("https://open.spotify.com/track/4uLU6hMCjMI75M1A2tKUQC")
        );
    }

    #[test]
    fn tunemymusic_and_russian_headers() {
        let csv = "Track name,Artist name,Album,Playlist name,Type,ISRC\n\
                   Группа крови,Кино,Группа крови,Мой рок,Playlist,\n";
        let (tracks, name) = parse_named(csv).unwrap();
        assert_eq!(tracks[0].title, "Группа крови");
        assert_eq!(tracks[0].isrc, None);
        assert_eq!(name.as_deref(), Some("Мой рок"));

        let semicolons = "Название;Исполнитель;Длительность\nСпокойная ночь;Кино;6:06\n";
        let tracks = parse(semicolons).unwrap();
        assert_eq!(tracks[0].artists, ["Кино"]);
        assert_eq!(tracks[0].duration, Some(366.0));
    }

    #[test]
    fn not_a_table() {
        assert!(parse("Daft Punk - One More Time\nNumb\n").is_none());
        assert!(parse("Title\nNumb\n").is_none());
    }
}
