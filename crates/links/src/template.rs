//! File names for downloads, from a user template such as `{artist} - {title}`.

/// The default template: "Artist - Title".
pub const DEFAULT_TEMPLATE: &str = "{artist} - {title}";

/// The default template for tracks of an album: a folder per artist and album.
pub const DEFAULT_ALBUM_TEMPLATE: &str = "{album_artist}/{album}/{track} {title}";

/// Longest file stem, in bytes: well below every file system's limit, with room for the
/// extension and a " (2)" suffix.
const MAX_STEM: usize = 150;

/// Values the placeholders `{artist}`, `{title}`, `{album}`, `{album_artist}`, `{track}`
/// (two digits) and `{year}` stand for.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileNameFields<'a> {
    pub artist: Option<&'a str>,
    pub title: Option<&'a str>,
    pub album: Option<&'a str>,
    /// Falls back to `artist`.
    pub album_artist: Option<&'a str>,
    pub track: Option<u32>,
    pub year: Option<u16>,
}

/// Splits a template into literal texts and field values: `texts[i]` comes before
/// `values[i]`, and `texts` has one more element than `values`.
fn parse(template: &str, fields: &FileNameFields<'_>) -> (Vec<String>, Vec<String>) {
    let mut texts = vec![String::new()];
    let mut values = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start..].find('}') else {
            break;
        };
        let value = match &rest[start + 1..start + len] {
            "artist" => Some(fields.artist.unwrap_or("").to_owned()),
            "title" => Some(fields.title.unwrap_or("").to_owned()),
            "album" => Some(fields.album.unwrap_or("").to_owned()),
            "album_artist" => Some(
                fields
                    .album_artist
                    .or(fields.artist)
                    .unwrap_or("")
                    .to_owned(),
            ),
            "track" => Some(fields.track.map(|n| format!("{n:02}")).unwrap_or_default()),
            "year" => Some(fields.year.map(|y| y.to_string()).unwrap_or_default()),
            _ => None,
        };
        let text = texts.last_mut().expect("starts with one text");
        match value {
            Some(value) => {
                text.push_str(&rest[..start]);
                values.push(value.trim().to_owned());
                texts.push(String::new());
            }
            // Unknown placeholders stay as written.
            None => text.push_str(&rest[..start + len + 1]),
        }
        rest = &rest[start + len + 1..];
    }
    texts
        .last_mut()
        .expect("starts with one text")
        .push_str(rest);
    (texts, values)
}

/// Renders a template. A placeholder without a value disappears together with the text
/// around it: what's kept between two values is the text that followed the first, so
/// `{artist} - {title}` without an artist is just the title.
fn render(template: &str, fields: &FileNameFields<'_>) -> String {
    let (texts, values) = parse(template, fields);
    if values.is_empty() {
        return texts.concat();
    }
    let present: Vec<usize> = (0..values.len())
        .filter(|&i| !values[i].is_empty())
        .collect();
    let (Some(&first), Some(&last)) = (present.first(), present.last()) else {
        return String::new();
    };
    let mut out = String::new();
    if first == 0 {
        out.push_str(&texts[0]);
    }
    for (n, &i) in present.iter().enumerate() {
        out.push_str(&values[i]);
        if n + 1 < present.len() {
            out.push_str(&texts[i + 1]);
        }
    }
    if last == values.len() - 1 {
        out.push_str(&texts[values.len()]);
    }
    out
}

/// Makes a string safe as a file name on macOS, Windows and Linux.
pub fn sanitize_file_name(name: &str) -> String {
    let mapped: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' => '-',
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut clean = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
    // Leading dots hide files; trailing dots and spaces are dropped by Windows.
    clean = clean
        .trim_start_matches(['.', ' '])
        .trim_end_matches(['.', ' '])
        .to_owned();
    if clean.len() > MAX_STEM {
        let mut end = MAX_STEM;
        while !clean.is_char_boundary(end) {
            end -= 1;
        }
        clean.truncate(end);
        clean = clean.trim_end_matches(['.', ' ']).to_owned();
    }
    let stem = clean.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        clean.push('_');
    }
    clean
}

/// The file name (without extension) for a download.
pub fn file_stem(template: &str, fields: &FileNameFields<'_>) -> String {
    let template = if template.trim().is_empty() {
        DEFAULT_TEMPLATE
    } else {
        template
    };
    let stem = sanitize_file_name(&render(template, fields));
    if !stem.is_empty() {
        return stem;
    }
    let title = sanitize_file_name(fields.title.unwrap_or(""));
    if title.is_empty() {
        "Untitled".to_owned()
    } else {
        title
    }
}

/// Where a download goes, relative to the download folder: the folders a template's `/`
/// separates (each rendered and made safe on its own; empty ones are left out) and the file
/// name without extension.
pub fn relative_path(template: &str, fields: &FileNameFields<'_>) -> (Vec<String>, String) {
    let template = if template.trim().is_empty() {
        DEFAULT_TEMPLATE
    } else {
        template
    };
    let mut segments: Vec<&str> = template.split(['/', '\\']).collect();
    let name = segments.pop().unwrap_or_default();
    let folders = segments
        .into_iter()
        .map(|segment| sanitize_file_name(&render(segment, fields)))
        .filter(|folder| !folder.is_empty())
        .collect();
    (folders, file_stem(name, fields))
}

/// `stem`, or the first of `stem (2)`, `stem (3)`, ... that `taken` says is free, so a
/// download never overwrites a file (or another download's name).
pub fn unique_stem(stem: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(stem) {
        return stem.to_owned();
    }
    (2..)
        .map(|n| format!("{stem} ({n})"))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| stem.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields<'a>(artist: Option<&'a str>, title: Option<&'a str>) -> FileNameFields<'a> {
        FileNameFields {
            artist,
            title,
            album: Some("Album"),
            year: Some(1999),
            ..FileNameFields::default()
        }
    }

    #[test]
    fn album_tracks_get_folders_and_numbers() {
        let track = FileNameFields {
            artist: Some("VILLIAN, madk1d"),
            title: Some("Song: Part 2"),
            album: Some("Night/Day"),
            album_artist: Some("VILLIAN"),
            track: Some(3),
            year: Some(2024),
        };
        assert_eq!(
            relative_path(DEFAULT_ALBUM_TEMPLATE, &track),
            (
                vec!["VILLIAN".to_owned(), "Night-Day".to_owned()],
                "03 Song_ Part 2".to_owned()
            )
        );
        // Missing values drop their folder; the album artist falls back to the artist.
        let loose = FileNameFields {
            album_artist: None,
            album: None,
            ..track
        };
        assert_eq!(
            relative_path(DEFAULT_ALBUM_TEMPLATE, &loose),
            (
                vec!["VILLIAN, madk1d".to_owned()],
                "03 Song_ Part 2".to_owned()
            )
        );
        assert_eq!(
            relative_path("{artist} - {title}", &track),
            (Vec::new(), "VILLIAN, madk1d - Song_ Part 2".to_owned())
        );
        assert_eq!(
            relative_path("{year}\\{title}", &track),
            (vec!["2024".to_owned()], "Song_ Part 2".to_owned())
        );
    }

    #[test]
    fn renders_the_default_template() {
        let name = file_stem(
            DEFAULT_TEMPLATE,
            &fields(Some("Кино"), Some("Группа крови")),
        );
        assert_eq!(name, "Кино - Группа крови");
    }

    #[test]
    fn missing_values_take_their_separator_with_them() {
        assert_eq!(
            file_stem(DEFAULT_TEMPLATE, &fields(None, Some("Song"))),
            "Song"
        );
        assert_eq!(
            file_stem(DEFAULT_TEMPLATE, &fields(Some("Band"), None)),
            "Band"
        );
        let no_album = FileNameFields {
            artist: Some("A"),
            title: Some("T"),
            ..FileNameFields::default()
        };
        assert_eq!(
            file_stem("{artist} - {album} - {title}", &no_album),
            "A - T"
        );
        assert_eq!(file_stem("{year} {artist} - {title}", &no_album), "A - T");
        assert_eq!(file_stem("[{year}] {title}", &no_album), "T");
        assert_eq!(
            file_stem("{artist}/{year} - {title}", &fields(Some("A"), Some("T"))),
            "A-1999 - T"
        );
    }

    #[test]
    fn unknown_placeholders_stay_and_empty_templates_use_the_default() {
        assert_eq!(
            file_stem("{genre} {title}", &fields(None, Some("T"))),
            "{genre} T"
        );
        assert_eq!(file_stem("  ", &fields(Some("A"), Some("T"))), "A - T");
        assert_eq!(
            file_stem("{artist}", &FileNameFields::default()),
            "Untitled"
        );
    }

    #[test]
    fn sanitizes_for_every_file_system() {
        assert_eq!(sanitize_file_name("AC/DC: Live?"), "AC-DC_ Live_");
        assert_eq!(sanitize_file_name("  ..hidden  "), "hidden");
        assert_eq!(sanitize_file_name("trailing dots..."), "trailing dots");
        assert_eq!(sanitize_file_name("tab\there\nnew"), "tab here new");
        assert_eq!(sanitize_file_name("CON"), "CON_");
        assert_eq!(sanitize_file_name("com1.txt"), "com1.txt_");
        assert_eq!(sanitize_file_name("Console"), "Console");
    }

    #[test]
    fn taken_names_get_a_number() {
        assert_eq!(unique_stem("A - T", |_| false), "A - T");
        let taken = ["A - T", "A - T (2)"];
        assert_eq!(unique_stem("A - T", |s| taken.contains(&s)), "A - T (3)");
    }

    #[test]
    fn long_names_are_cut_on_a_character_boundary() {
        let long = "й".repeat(200);
        let cut = sanitize_file_name(&long);
        assert!(cut.len() <= MAX_STEM);
        assert!(cut.chars().all(|c| c == 'й'));
    }
}
