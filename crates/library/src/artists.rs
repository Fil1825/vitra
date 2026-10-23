//! Tracks with several artists. A file may list them as separate tag values (several
//! `ARTIST` comments, an ID3v2.4 `TPE1` with null separators, an `ARTISTS` tag) or in one
//! credit ("A feat. B", "A, B", "A x B"). Credits are split cautiously: names with a
//! separator in them ("Tyler, The Creator") are kept whole, and the user can turn splitting
//! off. Each track's artists are stored in order in `track_artists`; the first is the main
//! one, which also groups the track's album when the album has no artist of its own.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::search::fold;
use crate::{Library, Result};

/// How one artist credit is split into artists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ArtistSplitting {
    pub enabled: bool,
    /// Names with a separator in them that are one artist, besides [`BUILTIN_EXCEPTIONS`].
    pub exceptions: Vec<String>,
}

impl Default for ArtistSplitting {
    fn default() -> Self {
        Self {
            enabled: true,
            exceptions: Vec::new(),
        }
    }
}

/// Well-known artists whose names contain a separator.
pub const BUILTIN_EXCEPTIONS: &[&str] = &[
    "Tyler, The Creator",
    "Earth, Wind & Fire",
    "Crosby, Stills, Nash & Young",
    "Crosby, Stills & Nash",
    "Emerson, Lake & Palmer",
    "Peter, Paul and Mary",
    "Blood, Sweat & Tears",
    "Simon & Garfunkel",
    "Hall & Oates",
    "Daryl Hall & John Oates",
    "Mumford & Sons",
    "Kool & the Gang",
    "Belle & Sebastian",
    "Iron & Wine",
    "Ike & Tina Turner",
    "Sam & Dave",
    "Brooks & Dunn",
    "Above & Beyond",
    "Chase & Status",
    "Years & Years",
    "Angus & Julia Stone",
    "Matt & Kim",
    "She & Him",
    "Of Mice & Men",
    "Love & Rockets",
    "Echo & the Bunnymen",
    "Siouxsie & the Banshees",
    "Hootie & the Blowfish",
    "Huey Lewis & the News",
    "Tom Petty & the Heartbreakers",
    "Bob Marley & the Wailers",
    "Nick Cave & the Bad Seeds",
    "Sly & the Family Stone",
    "Derek & the Dominos",
    "Marina & the Diamonds",
    "Florence & the Machine",
    "Prince & the Revolution",
    "Eric B. & Rakim",
    "DJ Jazzy Jeff & the Fresh Prince",
    "Captain & Tennille",
    "Peaches & Herb",
    "Ashford & Simpson",
    "Zager & Evans",
    "Big & Rich",
    "Dan + Shay",
    "Florence + the Machine",
];

/// Where one credit splits: `(text, ignore case)`. Longer forms come first so " feat. "
/// wins over " feat ". Only a lowercase " x " counts, so "Lil Nas X x Y" splits after the X.
const SEPARATORS: &[(&str, bool)] = &[
    (" (featuring ", true),
    (" (feat. ", true),
    (" (feat ", true),
    (" (ft. ", true),
    (" [feat. ", true),
    (" [ft. ", true),
    (" featuring ", true),
    (" feat. ", true),
    (" feat ", true),
    (" ft. ", true),
    (" ft ", true),
    (" при участии ", true),
    (" при уч. ", true),
    (", ", false),
    (" & ", false),
    (" x ", false),
    (" × ", false),
    // Cyrillic "х", as people type it in Russian credits.
    (" х ", false),
    ("; ", false),
];

/// The byte length of `pattern` if `text` starts with it.
fn starts_with(text: &str, pattern: &str, ignore_case: bool) -> Option<usize> {
    if !ignore_case {
        return text.starts_with(pattern).then_some(pattern.len());
    }
    let mut chars = text.char_indices();
    for expected in pattern.chars() {
        let (_, actual) = chars.next()?;
        if !actual.to_lowercase().eq(expected.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(index, _)| index))
}

/// Byte ranges of `credit` taken by exceptions, as whole words, left to right.
fn protected_ranges(credit: &str, exceptions: &[&str]) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut previous: Option<char> = None;
    let mut index = 0;
    while index < credit.len() {
        let at_word_start = previous.is_none_or(|c| !c.is_alphanumeric());
        let longest = at_word_start
            .then(|| {
                exceptions
                    .iter()
                    .filter_map(|name| starts_with(&credit[index..], name, true))
                    .filter(|len| {
                        credit[index + len..]
                            .chars()
                            .next()
                            .is_none_or(|c| !c.is_alphanumeric())
                    })
                    .max()
            })
            .flatten();
        if let Some(len) = longest {
            ranges.push((index, index + len));
            previous = credit[..index + len].chars().next_back();
            index += len;
        } else {
            let c = credit[index..].chars().next().unwrap_or(' ');
            previous = Some(c);
            index += c.len_utf8();
        }
    }
    ranges
}

/// A part of a credit, without the brackets a "(feat. B)" leaves behind.
fn clean_part(part: &str) -> &str {
    let mut part = part.trim();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if part.ends_with(close) && !part.contains(open) {
            part = part[..part.len() - 1].trim_end();
        }
    }
    part
}

/// Splits one credit into artists, main artist first. Without a separator (or with
/// splitting off) it is one artist.
pub fn split_artists(credit: &str, splitting: &ArtistSplitting) -> Vec<String> {
    let credit = credit.trim();
    if credit.is_empty() {
        return Vec::new();
    }
    if !splitting.enabled {
        return vec![credit.to_owned()];
    }
    let exceptions: Vec<&str> = BUILTIN_EXCEPTIONS
        .iter()
        .copied()
        .chain(splitting.exceptions.iter().map(String::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    let protected = protected_ranges(credit, &exceptions);

    let mut parts = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < credit.len() {
        if let Some(&(_, end)) = protected.iter().find(|(from, _)| *from == index) {
            index = end;
            continue;
        }
        let separator = SEPARATORS.iter().find_map(|(pattern, ignore_case)| {
            starts_with(&credit[index..], pattern, *ignore_case)
        });
        match separator {
            Some(len) => {
                parts.push(&credit[start..index]);
                index += len;
                start = index;
            }
            None => index += credit[index..].chars().next().map_or(1, char::len_utf8),
        }
    }
    parts.push(&credit[start..]);

    let names = unique(parts.into_iter().map(clean_part));
    if names.is_empty() {
        vec![credit.to_owned()]
    } else {
        names
    }
}

/// Trimmed, non-empty names without repeats (ignoring case), in order.
fn unique<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut result = Vec::new();
    for name in names {
        let name = name.trim();
        let key = name.to_lowercase();
        if !name.is_empty() && !seen.contains(&key) {
            seen.push(key);
            result.push(name.to_owned());
        }
    }
    result
}

/// A track's artists: a list (several tag values, an `ARTISTS` tag, a site's list) as it
/// is, else its one credit split. Also returns the credit when it was split, so a change of
/// the setting can split it again.
pub(crate) fn artist_names(
    values: &[String],
    credit: Option<&str>,
    splitting: &ArtistSplitting,
) -> (Vec<String>, Option<String>) {
    let listed = unique(values.iter().map(String::as_str));
    if !listed.is_empty() {
        return (listed, None);
    }
    let credit = credit.map(str::trim).filter(|c| !c.is_empty());
    match credit {
        Some(credit) => (split_artists(credit, splitting), Some(credit.to_owned())),
        None => (Vec::new(), None),
    }
}

/// The artist an album is filed under: the main one of its album artist credit.
pub(crate) fn album_artist_name(
    credit: Option<&str>,
    splitting: &ArtistSplitting,
) -> Option<String> {
    credit.and_then(|credit| split_artists(credit, splitting).into_iter().next())
}

const SPLITTING_KEY: &str = "artistSplitting";

/// The splitting setting, kept in the database so every connection (the scanner's too)
/// splits the same way.
pub(crate) fn splitting(conn: &Connection) -> Result<ArtistSplitting> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM session WHERE key = ?",
            [SPLITTING_KEY],
            |row| row.get(0),
        )
        .optional()?;
    Ok(stored
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default())
}

/// The id of the artist called `name`, added if new (with its search entry).
pub(crate) fn artist_id(tx: &Transaction<'_>, name: &str) -> Result<i64> {
    let existing: Option<i64> = tx
        .query_row("SELECT id FROM artists WHERE name = ?", [name], |row| {
            row.get(0)
        })
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    tx.execute("INSERT INTO artists (name) VALUES (?)", [name])?;
    let id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO artist_search (rowid, name) VALUES (?1, ?2)",
        params![id, fold(name)],
    )?;
    Ok(id)
}

/// Replaces a track's artists, in order.
pub(crate) fn write_track_artists(tx: &Transaction<'_>, track_id: i64, ids: &[i64]) -> Result<()> {
    tx.execute("DELETE FROM track_artists WHERE track_id = ?", [track_id])?;
    for (position, id) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO track_artists (track_id, artist_id, position) VALUES (?1, ?2, ?3)",
            params![track_id, id, position as i64],
        )?;
    }
    Ok(())
}

/// One track as re-splitting sees it.
struct Stored {
    id: i64,
    credit: Option<String>,
    album_artist: Option<String>,
    album: Option<(i64, String)>,
    year: Option<u16>,
}

/// Splits every track's credit again (after the setting changed or the database was
/// upgraded) and files albums under their new main artist.
fn resplit(tx: &Transaction<'_>, splitting: &ArtistSplitting) -> Result<()> {
    let tracks: Vec<Stored> = {
        let mut statement = tx.prepare(
            "SELECT t.id, t.artist_credit, t.album_artist, t.album_id, al.title, t.year
             FROM tracks t LEFT JOIN albums al ON al.id = t.album_id",
        )?;
        statement
            .query_map([], |row| {
                let album_id: Option<i64> = row.get(3)?;
                let album_title: Option<String> = row.get(4)?;
                Ok(Stored {
                    id: row.get(0)?,
                    credit: row.get(1)?,
                    album_artist: row.get(2)?,
                    album: album_id.zip(album_title),
                    year: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?
    };
    for track in tracks {
        let names: Vec<String> = match &track.credit {
            Some(credit) => split_artists(credit, splitting),
            // A list from the tags stays as it is.
            None => {
                let mut statement = tx.prepare_cached(
                    "SELECT ar.name FROM track_artists ta JOIN artists ar ON ar.id = ta.artist_id
                     WHERE ta.track_id = ? ORDER BY ta.position",
                )?;
                statement
                    .query_map([track.id], |row| row.get(0))?
                    .collect::<rusqlite::Result<_>>()?
            }
        };
        let ids = names
            .iter()
            .map(|name| artist_id(tx, name))
            .collect::<Result<Vec<_>>>()?;
        write_track_artists(tx, track.id, &ids)?;
        let album_artist = album_artist_name(track.album_artist.as_deref(), splitting)
            .or_else(|| names.first().cloned());
        let album_artist_id = album_artist
            .as_deref()
            .map(|name| artist_id(tx, name))
            .transpose()?;
        if let Some((old_album, title)) = &track.album {
            let album = crate::tracks::album_id(tx, Some(title), album_artist_id, track.year)?
                .unwrap_or(*old_album);
            if album != *old_album {
                // The album's cover comes along until the next scan looks again.
                tx.execute(
                    "UPDATE albums SET cover = (SELECT cover FROM albums WHERE id = ?2)
                     WHERE id = ?1 AND cover IS NULL",
                    params![album, old_album],
                )?;
                tx.execute(
                    "UPDATE tracks SET album_id = ?2 WHERE id = ?1",
                    params![track.id, album],
                )?;
            }
        }
        tx.execute(
            "UPDATE tracks SET artist_id = ?2 WHERE id = ?1",
            params![track.id, ids.first()],
        )?;
        tx.execute(
            "UPDATE track_search SET artist = ?2, album_artist = ?3 WHERE rowid = ?1",
            params![
                track.id,
                (!names.is_empty()).then(|| fold(&names.join(" "))),
                album_artist.as_deref().map(fold)
            ],
        )?;
    }
    crate::tracks::prune(tx)
}

/// Set by migration 4: tracks stored before artists were split need splitting once.
pub(crate) const RESPLIT_PENDING_KEY: &str = "artists.resplitPending";

impl Library {
    pub fn artist_splitting(&self) -> Result<ArtistSplitting> {
        splitting(&self.conn)
    }

    /// Changes how credits are split and re-splits the library; returns whether anything
    /// changed.
    pub fn set_artist_splitting(&mut self, value: &ArtistSplitting) -> Result<bool> {
        if self.artist_splitting()? == *value {
            return Ok(false);
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO session (key, value) VALUES (?1, ?2)",
            params![SPLITTING_KEY, serde_json::to_string(value)?],
        )?;
        resplit(&tx, value)?;
        tx.commit()?;
        Ok(true)
    }

    /// Splits the credits of tracks stored before artists were split (once, after the
    /// upgrade).
    pub(crate) fn resplit_if_pending(&mut self) -> Result<()> {
        let pending: bool = self.conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM session WHERE key = ?)",
            [RESPLIT_PENDING_KEY],
            |row| row.get(0),
        )?;
        if !pending {
            return Ok(());
        }
        let tx = self.conn.transaction()?;
        let splitting = splitting(&tx)?;
        resplit(&tx, &splitting)?;
        tx.execute("DELETE FROM session WHERE key = ?", [RESPLIT_PENDING_KEY])?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(credit: &str) -> Vec<String> {
        split_artists(credit, &ArtistSplitting::default())
    }

    #[test]
    fn splits_common_credits() {
        assert_eq!(split("VILLIAN, madk1d"), ["VILLIAN", "madk1d"]);
        assert_eq!(
            split("greyrock, tewiq, madk1d"),
            ["greyrock", "tewiq", "madk1d"]
        );
        assert_eq!(split("A feat. B"), ["A", "B"]);
        assert_eq!(split("A Feat. B & C"), ["A", "B", "C"]);
        assert_eq!(split("A ft. B"), ["A", "B"]);
        assert_eq!(split("A (feat. B)"), ["A", "B"]);
        assert_eq!(split("A [ft. B, C]"), ["A", "B", "C"]);
        assert_eq!(split("A featuring B"), ["A", "B"]);
        assert_eq!(split("A x B"), ["A", "B"]);
        assert_eq!(split("Скриптонит х 104"), ["Скриптонит", "104"]);
        assert_eq!(split("Баста при участии Ноггано"), ["Баста", "Ноггано"]);
        assert_eq!(split("A; B"), ["A", "B"]);
        assert_eq!(split("  Solo Artist  "), ["Solo Artist"]);
        assert!(split("   ").is_empty());
    }

    #[test]
    fn keeps_names_that_contain_separators() {
        assert_eq!(split("Tyler, The Creator"), ["Tyler, The Creator"]);
        assert_eq!(
            split("Tyler, The Creator & Kali Uchis"),
            ["Tyler, The Creator", "Kali Uchis"]
        );
        assert_eq!(split("tyler, the creator"), ["tyler, the creator"]);
        assert_eq!(split("Earth, Wind & Fire"), ["Earth, Wind & Fire"]);
        assert_eq!(split("Simon & Garfunkel"), ["Simon & Garfunkel"]);
        assert_eq!(
            split("Lil Nas X x Billy Ray Cyrus"),
            ["Lil Nas X", "Billy Ray Cyrus"]
        );
        // Only whole names are protected.
        assert_eq!(split("Simon & Garfunkelish"), ["Simon", "Garfunkelish"]);
        // No comma without a space: "10,000 Maniacs" is one name.
        assert_eq!(split("10,000 Maniacs"), ["10,000 Maniacs"]);
        assert_eq!(split("AC/DC"), ["AC/DC"]);
        assert_eq!(split("Maximilian"), ["Maximilian"], "no split inside words");
    }

    #[test]
    fn user_exceptions_and_turning_splitting_off() {
        let custom = ArtistSplitting {
            enabled: true,
            exceptions: vec!["Каспийский Груз, Ёлка".into(), " ".into()],
        };
        assert_eq!(
            split_artists("Каспийский Груз, Ёлка", &custom),
            ["Каспийский Груз, Ёлка"]
        );
        let off = ArtistSplitting {
            enabled: false,
            exceptions: Vec::new(),
        };
        assert_eq!(split_artists("A, B", &off), ["A, B"]);
    }

    #[test]
    fn repeated_names_count_once() {
        assert_eq!(split("A, a & B"), ["A", "B"]);
    }

    #[test]
    fn tag_lists_win_over_splitting() {
        let splitting = ArtistSplitting::default();
        let (names, credit) = artist_names(
            &["Tyler".into(), "The Creator".into()],
            Some("Tyler"),
            &splitting,
        );
        assert_eq!(names, ["Tyler", "The Creator"]);
        assert_eq!(credit, None);
        // One listed value is one artist too ("ARTISTS=Foo & Bar").
        let (names, credit) = artist_names(&["Foo & Bar".into()], Some("x"), &splitting);
        assert_eq!((names, credit), (vec!["Foo & Bar".to_owned()], None));
        let (names, credit) = artist_names(&[], Some("A, B"), &splitting);
        assert_eq!(names, ["A", "B"]);
        assert_eq!(credit.as_deref(), Some("A, B"));
        assert_eq!(artist_names(&[], None, &splitting), (Vec::new(), None));
        assert_eq!(
            album_artist_name(Some("VILLIAN, madk1d"), &splitting).as_deref(),
            Some("VILLIAN")
        );
    }
}
