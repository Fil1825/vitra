//! Titles and names as matching compares them: case, accents, ё/е and punctuation don't
//! count; Cyrillic can be compared in Latin letters ("Кино" and "Kino"); what makes a
//! recording another one (live, remix, acoustic…) is kept apart from the words that don't
//! ("Remastered 2011", "feat. X", "Official Video").

use std::collections::BTreeSet;

/// Lower case, ё → е, accents off, everything but letters and digits a single space.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars().flat_map(char::to_lowercase) {
        let c = match c {
            'ё' => 'е',
            'й' => 'и',
            c => strip_accent(c),
        };
        if c.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(c);
        } else if c == '\'' || c == '’' || c == '`' {
            // "Don't" and "Dont" are the same word.
        } else {
            space = true;
        }
    }
    out
}

/// Common accented Latin letters without their accents.
fn strip_accent(c: char) -> char {
    match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' | 'đ' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'ğ' => 'g',
        'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => 'i',
        'ł' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' | 'ș' => 's',
        'ť' | 'ţ' | 'ț' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        // Ukrainian and Belarusian letters as their Russian neighbours.
        'і' | 'ї' => 'и',
        'є' => 'е',
        'ў' => 'у',
        'ґ' => 'г',
        c => c,
    }
}

/// Cyrillic written in Latin letters, the way artists usually spell themselves abroad
/// ("кино" → "kino", "земфира" → "zemfira"); expects [`fold`]ed text.
pub fn latin(folded: &str) -> String {
    let mut out = String::with_capacity(folded.len() + 8);
    for c in folded.chars() {
        let latin = match c {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "h",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "sch",
            'ъ' | 'ь' => "",
            'ы' => "y",
            'э' => "e",
            'ю' => "yu",
            'я' => "ya",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(latin);
    }
    out
}

/// What makes a recording another one: a live take is not the studio song.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Version {
    Live,
    Remix,
    Acoustic,
    Instrumental,
    Demo,
    Karaoke,
    SpedUp,
    Slowed,
    Cover,
}

/// Words in brackets or after " - " that tell a version apart (folded).
const VERSION_WORDS: &[(&str, Version)] = &[
    ("live", Version::Live),
    ("концерт", Version::Live),
    ("живое", Version::Live),
    ("remix", Version::Remix),
    ("rmx", Version::Remix),
    ("ремикс", Version::Remix),
    ("acoustic", Version::Acoustic),
    ("unplugged", Version::Acoustic),
    ("акустика", Version::Acoustic),
    ("акустическая", Version::Acoustic),
    ("instrumental", Version::Instrumental),
    ("минус", Version::Instrumental),
    ("demo", Version::Demo),
    ("karaoke", Version::Karaoke),
    ("караоке", Version::Karaoke),
    ("sped", Version::SpedUp),
    ("speed", Version::SpedUp),
    ("nightcore", Version::SpedUp),
    ("slowed", Version::Slowed),
    ("cover", Version::Cover),
    ("кавер", Version::Cover),
];

/// Words that only describe a release or a video (folded): such a part says nothing.
const NOISE_WORDS: &[&str] = &[
    "remaster",
    "remastered",
    "remasterd",
    "version",
    "versión",
    "edit",
    "radio",
    "single",
    "album",
    "mono",
    "stereo",
    "deluxe",
    "bonus",
    "explicit",
    "clean",
    "official",
    "video",
    "audio",
    "lyric",
    "lyrics",
    "visualizer",
    "music",
    "hd",
    "hq",
    "4k",
    "original",
    "mix",
    "extended",
    "edition",
    "anniversary",
    "mixed",
    "mv",
    "клип",
    "премьера",
    "official",
    "текст",
];

/// A title taken apart for matching.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Title {
    /// The words that name the song, [`fold`]ed.
    pub base: String,
    pub versions: BTreeSet<Version>,
    /// Artists credited in the title ("feat. X"), folded.
    pub featuring: Vec<String>,
}

/// Separators between names in an artist credit (folded text, so spaces around).
const ARTIST_SEPARATORS: &[&str] = &[
    " feat ",
    " ft ",
    " featuring ",
    " with ",
    " x ",
    " и ",
    " vs ",
    " and ",
];

/// The names in an artist credit, folded: "A feat. B & C" → ["a", "b", "c"].
pub fn artist_names(credit: &str) -> Vec<String> {
    // Split on punctuation separators before folding turns them into spaces.
    let mut parts: Vec<String> = credit
        .split([',', '&', ';', '/', '×', '+'])
        .map(fold)
        .collect();
    for separator in ARTIST_SEPARATORS {
        parts = parts
            .into_iter()
            .flat_map(|part| {
                let padded = format!(" {part} ");
                let pieces: Vec<String> = padded
                    .split(separator)
                    .map(|p| p.trim().to_owned())
                    .collect();
                // "Lil Nas X" isn't two people.
                if pieces.iter().any(String::is_empty) {
                    vec![part]
                } else {
                    pieces
                }
            })
            .collect();
    }
    parts.retain(|part| !part.is_empty());
    parts.dedup();
    parts
}

/// Takes `title` apart: brackets and " - …" parts that name a version or only describe the
/// release come off the name; "feat." credits go to [`Title::featuring`].
pub fn title(text: &str) -> Title {
    let mut result = Title::default();
    let text = text.replace(" – ", " - ").replace(" — ", " - ");
    // Bracketed parts.
    let mut name = String::new();
    let mut depth = 0usize;
    let mut extra = String::new();
    for c in text.chars() {
        match c {
            '(' | '[' | '{' | '（' => {
                if depth > 0 {
                    extra.push(' ');
                }
                depth += 1;
            }
            ')' | ']' | '}' | '）' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if take_extra(&extra, &mut result) {
                        // Brackets that are part of the name: "(I Can't Get No) Satisfaction".
                        name.push_str(&extra);
                    }
                    extra.clear();
                    name.push(' ');
                }
            }
            c if depth > 0 => extra.push(c),
            c => name.push(c),
        }
    }
    if depth > 0 && take_extra(&extra, &mut result) {
        name.push_str(&extra);
    }
    // " - Remastered 2011", " - Live at Wembley": a dash part that names a version or is
    // only noise; anything else stays ("Part 1 - The Beginning").
    let mut kept = String::new();
    for (index, part) in name.split(" - ").enumerate() {
        if index > 0 && is_extra(part) {
            take_extra(part, &mut result);
        } else {
            kept.push(' ');
            kept.push_str(part);
        }
    }
    // "Song feat. X & Y" without brackets (the credit read before folding loses the "&").
    if let Some((at, len)) = find_feat(&kept) {
        result.featuring.extend(artist_names(&kept[at + len..]));
        kept.truncate(at);
    }
    result.base = fold(&kept);
    result
}

/// Where an inline "feat." credit starts in `text`, and its marker's length.
fn find_feat(text: &str) -> Option<(usize, usize)> {
    const MARKERS: &[&str] = &[" feat. ", " feat ", " ft. ", " ft ", " featuring "];
    let bytes = text.as_bytes();
    (1..bytes.len()).find_map(|at| {
        MARKERS.iter().find_map(|marker| {
            let end = at + marker.len();
            (end <= bytes.len() && bytes[at..end].eq_ignore_ascii_case(marker.as_bytes()))
                .then_some((at, marker.len()))
        })
    })
}

/// Whether a " - …" part is a version or noise rather than part of the name.
fn is_extra(part: &str) -> bool {
    fold(part).split(' ').any(|w| {
        VERSION_WORDS.iter().any(|(v, _)| w.starts_with(v))
            || NOISE_WORDS.contains(&w)
            || matches!(w, "feat" | "ft" | "featuring")
    })
}

/// Reads a bracketed or " - …" part into `title`; `true` when it belongs to the name.
fn take_extra(extra: &str, title: &mut Title) -> bool {
    let folded = fold(extra);
    let mut words: Vec<&str> = folded.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        return false;
    }
    let mut credited = false;
    if let Some(at) = words
        .iter()
        .position(|w| matches!(*w, "feat" | "ft" | "featuring" | "with" | "при"))
    {
        title
            .featuring
            .extend(artist_names(&words[at + 1..].join(" ")));
        words.truncate(at);
        credited = true;
    }
    let mut versioned = false;
    for word in &words {
        // "Original Mix" is the record itself; another "mix" is a remix.
        if *word == "mix" && !words.contains(&"original") && !words.contains(&"extended") {
            title.versions.insert(Version::Remix);
            versioned = true;
        }
        for (prefix, version) in VERSION_WORDS {
            if word.starts_with(prefix) {
                title.versions.insert(*version);
                versioned = true;
            }
        }
    }
    let noise = words
        .iter()
        .all(|w| NOISE_WORDS.contains(w) || w.chars().all(|c| c.is_ascii_digit()));
    !credited && !versioned && !noise
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_case_accents_and_punctuation() {
        assert_eq!(fold("Beyoncé – Halo!"), "beyonce halo");
        assert_eq!(fold("Ёлка"), "елка");
        assert_eq!(fold("Don't Stop Me Now"), "dont stop me now");
        assert_eq!(fold("  AC/DC  "), "ac dc");
        assert_eq!(fold("Мумий Тролль"), "мумии тролль");
        assert_eq!(latin(&fold("Кино")), "kino");
        assert_eq!(latin(&fold("Земфира")), "zemfira");
        assert_eq!(latin(&fold("Сплин")), "splin");
    }

    #[test]
    fn versions_and_noise_come_off() {
        let remaster = title("Here Comes the Sun - Remastered 2009");
        assert_eq!(remaster.base, "here comes the sun");
        assert!(remaster.versions.is_empty());

        let live = title("Bohemian Rhapsody (Live Aid)");
        assert_eq!(live.base, "bohemian rhapsody");
        assert!(live.versions.contains(&Version::Live));

        let live_dash = title("Hotel California - Live On MTV, 1994");
        assert_eq!(live_dash.base, "hotel california");
        assert!(live_dash.versions.contains(&Version::Live));

        let feat = title("Bad Habits (feat. Bruno Mars)");
        assert_eq!(feat.base, "bad habits");
        assert_eq!(feat.featuring, ["bruno mars"]);

        let inline = title("Peaches feat. Daniel Caesar & Giveon");
        assert_eq!(inline.base, "peaches");
        assert_eq!(inline.featuring, ["daniel caesar", "giveon"]);

        let video = title("Numb [Official Music Video] (4K Remaster)");
        assert_eq!(video.base, "numb");

        let remix = title("Levitating (The Blessed Madonna Remix)");
        assert!(remix.versions.contains(&Version::Remix));
        let original = title("Strobe (Original Mix)");
        assert!(original.versions.is_empty());

        // Brackets that belong to the name stay.
        assert_eq!(
            title("(I Can't Get No) Satisfaction").base,
            "i cant get no satisfaction"
        );
        assert_eq!(title("Part 1 - The Beginning").base, "part 1 the beginning");

        let with_live = title("Yesterday (Live with Orchestra)");
        assert!(with_live.versions.contains(&Version::Live));
        assert_eq!(with_live.featuring, ["orchestra"]);
        assert_eq!(title("Song – Radio Edit").base, "song");

        let ru = title("Группа крови (Live)");
        assert_eq!(ru.base, "группа крови");
        assert!(ru.versions.contains(&Version::Live));
    }

    #[test]
    fn splits_credits() {
        assert_eq!(artist_names("Simon & Garfunkel"), ["simon", "garfunkel"]);
        assert_eq!(
            artist_names("Calvin Harris feat. Rihanna, Ne-Yo"),
            ["calvin harris", "rihanna", "ne yo"]
        );
        assert_eq!(artist_names("Баста и Тати"), ["баста", "тати"]);
        assert_eq!(artist_names("Lil Nas X"), ["lil nas x"]);
        assert_eq!(
            artist_names("Skrillex x Fred again.."),
            ["skrillex", "fred again"]
        );
    }
}
