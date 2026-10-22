//! Instant search over the FTS5 index (`track_search`).

use std::collections::HashSet;

use rusqlite::params;

use crate::model::{SearchResults, TrackFilter};
use crate::tracks::{
    ALBUM_SELECT, ARTIST_SELECT, TRACK_COLUMNS, TRACK_FROM, album_from_row, artist_from_row,
    track_from_row,
};
use crate::{Library, Result};

/// Albums and artists shown above the matching tracks.
const GROUP_LIMIT: usize = 12;

/// Text as it goes into (and is looked up in) the index. `unicode61`'s diacritic removal only
/// covers Latin letters, so Russian "ё" is folded to "е" here: people type "е" for both.
pub(crate) fn fold(text: &str) -> String {
    text.replace('ё', "е").replace('Ё', "Е")
}

/// Turns what the user typed into an FTS5 query in which every word must match the start of
/// a word ("aur bor" finds "Aurora Borealis"). Words are split the way the `unicode61`
/// tokenizer splits them and quoted, so FTS syntax characters in the input are harmless.
pub(crate) fn fts_query(input: &str) -> Option<String> {
    let input = fold(input);
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\"*"))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

impl Library {
    /// Tracks matching any field (title weighs most, lyrics least), plus albums matching by
    /// title or artist and artists matching by name.
    pub fn search(&self, input: &str, limit: usize) -> Result<SearchResults> {
        let Some(query) = fts_query(input) else {
            return Ok(SearchResults::default());
        };

        let mut statement = self.conn.prepare(&format!(
            "SELECT {TRACK_COLUMNS} FROM track_search, {TRACK_FROM}
             WHERE t.id = track_search.rowid AND track_search MATCH ?1
             ORDER BY bm25(track_search, 10.0, 5.0, 4.0, 2.0, 1.0) LIMIT ?2"
        ))?;
        let tracks = statement
            .query_map(params![query, limit as i64], track_from_row)?
            .collect::<rusqlite::Result<_>>()?;

        let album_ids = self.matching_ids(
            "t.album_id",
            &format!("{{album album_artist}} : ({query})"),
            "",
        )?;
        let mut albums = Vec::with_capacity(album_ids.len());
        for id in album_ids {
            albums.extend(self.conn.query_row(
                &format!("{ALBUM_SELECT} WHERE al.id = ? GROUP BY al.id"),
                [id],
                album_from_row,
            ));
        }

        // The `artist` column holds the track artist and `album_artist` the album's artist,
        // so each column maps back to a different artist id.
        let mut artist_ids =
            self.matching_ids("t.artist_id", &format!("{{artist}} : ({query})"), "")?;
        for id in self.matching_ids(
            "al.artist_id",
            &format!("{{album_artist}} : ({query})"),
            "JOIN albums al ON al.id = t.album_id",
        )? {
            if !artist_ids.contains(&id) {
                artist_ids.push(id);
            }
        }
        artist_ids.truncate(GROUP_LIMIT);
        let mut artists = Vec::with_capacity(artist_ids.len());
        for id in artist_ids {
            artists.extend(self.conn.query_row(
                &format!("{ARTIST_SELECT} WHERE ar.id = ?"),
                [id],
                artist_from_row,
            ));
        }

        Ok(SearchResults {
            tracks,
            albums,
            artists,
        })
    }

    /// Distinct non-null values of `column` over matching rows, best match first.
    fn matching_ids(&self, column: &str, query: &str, join: &str) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT {column} FROM track_search JOIN tracks t ON t.id = track_search.rowid {join}
             WHERE track_search MATCH ?1 AND {column} IS NOT NULL ORDER BY rank"
        ))?;
        let mut seen = HashSet::new();
        let mut ids = Vec::new();
        let mut rows = statement.query([query])?;
        while let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            if seen.insert(id) {
                ids.push(id);
                if ids.len() == GROUP_LIMIT {
                    break;
                }
            }
        }
        Ok(ids)
    }

    /// Ids of every track matching the query, for "play all results".
    pub fn search_track_ids(&self, input: &str) -> Result<Vec<i64>> {
        let Some(query) = fts_query(input) else {
            return Ok(Vec::new());
        };
        let mut statement = self.conn.prepare(
            "SELECT rowid FROM track_search WHERE track_search MATCH ?1
             ORDER BY bm25(track_search, 10.0, 5.0, 4.0, 2.0, 1.0)",
        )?;
        let ids = statement
            .query_map([query], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// The matching tracks themselves, in rank order.
    pub fn search_tracks(&self, input: &str) -> Result<Vec<crate::model::Track>> {
        self.tracks(&TrackFilter::Ids(self.search_track_ids(input)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::tests::{sample_library, scanned};

    fn titles(results: &SearchResults) -> Vec<&str> {
        results.tracks.iter().map(|t| t.title.as_str()).collect()
    }

    #[test]
    fn yo_and_ye_find_each_other() {
        let mut library = Library::open_in_memory().unwrap();
        library
            .upsert_tracks(
                None,
                &[
                    scanned("/r/1.mp3", "Ёлка", "Звёздный путь", "Альбом", 1),
                    scanned("/r/2.mp3", "Елочка", "Кто-то", "Альбом", 2),
                ],
            )
            .unwrap();
        let found = |q: &str| {
            let mut titles: Vec<String> = library
                .search(q, 10)
                .unwrap()
                .tracks
                .into_iter()
                .map(|t| t.title)
                .collect();
            titles.sort();
            titles
        };
        assert_eq!(found("елк"), ["Ёлка"]);
        assert_eq!(found("ёлоч"), ["Елочка"]);
        assert_eq!(found("звездный"), ["Ёлка"]);
        // Names are shown as written.
        assert_eq!(found("ЁЛКА"), ["Ёлка"]);
    }

    #[test]
    fn builds_prefix_queries_and_drops_syntax() {
        assert_eq!(fts_query("  "), None);
        assert_eq!(fts_query("*\"()"), None);
        assert_eq!(fts_query("aur bor"), Some("\"aur\"* \"bor\"*".into()));
        assert_eq!(
            fts_query("AC/DC: \"Back\" NEAR(x)"),
            Some("\"AC\"* \"DC\"* \"Back\"* \"NEAR\"* \"x\"*".into())
        );
    }

    #[test]
    fn matches_word_prefixes_in_any_field() {
        let library = sample_library();
        assert_eq!(titles(&library.search("fir", 10).unwrap()), ["First"]);
        // Artist and album words.
        let results = library.search("geog", 10).unwrap();
        assert_eq!(results.tracks.len(), 2);
        assert_eq!(results.albums.len(), 1);
        assert_eq!(results.albums[0].title, "Geogaddi");
        // Every word has to match.
        assert_eq!(
            titles(&library.search("boards sec", 10).unwrap()),
            ["Second"]
        );
        assert!(
            library
                .search("boards nothing", 10)
                .unwrap()
                .tracks
                .is_empty()
        );
    }

    #[test]
    fn ignores_case_and_diacritics_in_latin_and_cyrillic() {
        let library = sample_library();
        assert_eq!(titles(&library.search("CAFE", 10).unwrap()), ["Café Song"]);
        assert_eq!(
            titles(&library.search("amelie", 10).unwrap()),
            ["Café Song"]
        );
        let results = library.search("аквар", 10).unwrap();
        assert_eq!(titles(&results), ["Aurora"]);
        assert_eq!(results.artists.len(), 1);
        assert_eq!(results.artists[0].name, "Аквариум");
        assert_eq!(titles(&library.search("ИХТИО", 10).unwrap()), ["Aurora"]);
    }

    #[test]
    fn title_matches_rank_above_other_fields() {
        let mut library = Library::open_in_memory().unwrap();
        library
            .upsert_tracks(
                None,
                &[
                    scanned("/1.mp3", "Something", "Night Riders", "Roads", 1),
                    scanned("/2.mp3", "Night", "Someone", "Other", 1),
                ],
            )
            .unwrap();
        assert_eq!(
            titles(&library.search("night", 10).unwrap()),
            ["Night", "Something"]
        );
    }

    #[test]
    fn finds_artists_by_album_artist() {
        let mut library = Library::open_in_memory().unwrap();
        let mut track = scanned("/v/1.mp3", "One", "Guest", "Hits", 1);
        track.metadata.album_artist = Some("Various Artists".into());
        library.upsert_tracks(None, &[track]).unwrap();

        let results = library.search("vari", 10).unwrap();
        assert_eq!(
            results
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            ["Various Artists"]
        );
        assert_eq!(results.albums.len(), 1);
        let guest = library.search("guest", 10).unwrap();
        assert_eq!(guest.artists[0].name, "Guest");
        assert!(guest.albums.is_empty());
    }

    #[test]
    fn searching_after_a_rescan_finds_the_new_tags_only() {
        let mut library = sample_library();
        library
            .upsert_tracks(
                None,
                &[scanned("/m/b/1.flac", "Renamed", "Boards", "Geogaddi", 1)],
            )
            .unwrap();
        assert!(library.search("first", 10).unwrap().tracks.is_empty());
        assert_eq!(titles(&library.search("renamed", 10).unwrap()), ["Renamed"]);
        assert_eq!(library.search_track_ids("boards").unwrap().len(), 2);
    }
}
