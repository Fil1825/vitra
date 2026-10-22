//! [LRCLIB](https://lrclib.net), a free lyrics database: building its request URLs and
//! choosing among its answers. The app runs the requests (with curl, like its other downloads).
//!
//! `GET /api/get` looks a track up by its exact signature (title, artist, album, duration
//! within ±2 s) and answers 404 when it has none; `GET /api/search` answers a list.

use serde::Deserialize;

pub const API: &str = "https://lrclib.net/api";

/// What is known about the track being looked up.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Query {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
}

/// One LRCLIB record.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Record {
    pub id: u64,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub duration: Option<f64>,
    /// Known to have no words: as good as an answer, there's nothing to show.
    pub instrumental: bool,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

impl Record {
    /// The best text it has: synced lyrics (LRC), else plain ones.
    pub fn text(&self) -> Option<&str> {
        [&self.synced_lyrics, &self.plain_lyrics]
            .into_iter()
            .flatten()
            .map(|text| text.as_str())
            .find(|text| !text.trim().is_empty())
    }
}

/// Percent-encodes a query value. Everything but unreserved ASCII is encoded, so the URL is
/// plain ASCII on the command line (Windows hands non-ASCII arguments to curl in the ANSI
/// code page, which would mangle Cyrillic titles).
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn url(endpoint: &str, params: &[(&str, &str)]) -> String {
    let query: Vec<String> = params
        .iter()
        .map(|(key, value)| format!("{key}={}", encode(value.trim())))
        .collect();
    format!("{API}/{endpoint}?{}", query.join("&"))
}

fn known(value: Option<&String>) -> Option<&str> {
    value.map(|v| v.trim()).filter(|v| !v.is_empty())
}

impl Query {
    /// The exact lookup; it needs the artist and the duration.
    pub fn get_url(&self) -> Option<String> {
        let artist = known(self.artist.as_ref())?;
        let duration = self.duration.filter(|d| *d > 0.0)?;
        if self.title.trim().is_empty() {
            return None;
        }
        let duration = format!("{}", duration.round());
        let mut params = vec![("track_name", self.title.as_str()), ("artist_name", artist)];
        if let Some(album) = known(self.album.as_ref()) {
            params.push(("album_name", album));
        }
        params.push(("duration", &duration));
        Some(url("get", &params))
    }

    /// The fuzzy search, by title and artist (or the title alone).
    pub fn search_url(&self) -> Option<String> {
        if self.title.trim().is_empty() {
            return None;
        }
        Some(match known(self.artist.as_ref()) {
            Some(artist) => url(
                "search",
                &[("track_name", self.title.as_str()), ("artist_name", artist)],
            ),
            None => url("search", &[("q", self.title.as_str())]),
        })
    }

    /// The search result that fits: same title (ignoring case), duration within 3 s when both
    /// are known; synced lyrics beat plain ones, then the closest duration wins.
    pub fn best<'a>(&self, records: &'a [Record]) -> Option<&'a Record> {
        let title = comparable(&self.title);
        let off = |record: &Record| match (self.duration, record.duration) {
            (Some(ours), Some(theirs)) => (ours - theirs).abs(),
            _ => 0.0,
        };
        records
            .iter()
            .filter(|record| comparable(&record.track_name) == title && off(record) <= 3.0)
            .filter(|record| record.instrumental || record.text().is_some())
            .min_by(|a, b| {
                // `false` sorts first.
                let unsynced = |r: &Record| r.synced_lyrics.as_deref().is_none_or(str::is_empty);
                unsynced(a)
                    .cmp(&unsynced(b))
                    .then(off(a).total_cmp(&off(b)))
            })
    }
}

fn comparable(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// The answer to `/api/get`.
pub fn parse_record(json: &str) -> Option<Record> {
    serde_json::from_str(json).ok()
}

/// The answer to `/api/search`.
pub fn parse_search(json: &str) -> Vec<Record> {
    serde_json::from_str(json).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> Query {
        Query {
            title: "Группа крови".into(),
            artist: Some("Кино".into()),
            album: Some("Группа крови".into()),
            duration: Some(286.4),
        }
    }

    #[test]
    fn builds_ascii_urls() {
        assert_eq!(
            query().get_url().unwrap(),
            "https://lrclib.net/api/get?track_name=%D0%93%D1%80%D1%83%D0%BF%D0%BF%D0%B0%20\
             %D0%BA%D1%80%D0%BE%D0%B2%D0%B8&artist_name=%D0%9A%D0%B8%D0%BD%D0%BE&album_name=\
             %D0%93%D1%80%D1%83%D0%BF%D0%BF%D0%B0%20%D0%BA%D1%80%D0%BE%D0%B2%D0%B8&duration=286"
        );
        let english = Query {
            title: "Rock & Roll (Live)".into(),
            artist: Some("Led Zeppelin".into()),
            album: None,
            duration: Some(220.0),
        };
        assert_eq!(
            english.get_url().unwrap(),
            "https://lrclib.net/api/get?track_name=Rock%20%26%20Roll%20%28Live%29\
             &artist_name=Led%20Zeppelin&duration=220"
        );
        assert_eq!(
            english.search_url().unwrap(),
            "https://lrclib.net/api/search?track_name=Rock%20%26%20Roll%20%28Live%29\
             &artist_name=Led%20Zeppelin"
        );
    }

    #[test]
    fn the_exact_lookup_needs_artist_and_duration() {
        let mut q = query();
        q.duration = None;
        assert_eq!(q.get_url(), None);
        let mut q = query();
        q.artist = Some(" ".into());
        assert_eq!(q.get_url(), None);
        assert_eq!(
            q.search_url().unwrap(),
            format!("{API}/search?q={}", encode("Группа крови"))
        );
        assert_eq!(Query::default().search_url(), None);
    }

    #[test]
    fn reads_records() {
        let json = r#"{"id":3396226,"trackName":"I Want to Live","artistName":"Borislav Slavov",
            "albumName":"Baldur's Gate 3","duration":233,"instrumental":false,
            "plainLyrics":"I feel your breath","syncedLyrics":"[00:17.12] I feel your breath"}"#;
        let record = parse_record(json).unwrap();
        assert_eq!(record.id, 3396226);
        assert_eq!(record.duration, Some(233.0));
        assert_eq!(record.text(), Some("[00:17.12] I feel your breath"));
        assert_eq!(
            parse_record(r#"{"code":404,"name":"TrackNotFound"}"#)
                .unwrap()
                .text(),
            None
        );
        assert!(parse_search("[]").is_empty());
        assert!(parse_search("not json").is_empty());
        let plain = Record {
            plain_lyrics: Some("words".into()),
            synced_lyrics: Some(String::new()),
            ..Record::default()
        };
        assert_eq!(plain.text(), Some("words"));
    }

    #[test]
    fn picks_the_fitting_search_result() {
        let record = |id, title: &str, duration, synced: bool| Record {
            id,
            track_name: title.into(),
            duration: Some(duration),
            plain_lyrics: Some("words".into()),
            synced_lyrics: synced.then(|| "[00:01.00]words".into()),
            ..Record::default()
        };
        let records = [
            record(1, "Группа крови", 286.0, false),
            record(2, "группа  крови", 288.0, true),
            record(3, "Группа крови", 300.0, true),
            record(4, "Группа крови (Live)", 286.0, true),
            record(5, "Группа крови", 286.0, true),
        ];
        // Synced first, then the closest duration; 3 is too long, 4 another title.
        assert_eq!(query().best(&records).map(|r| r.id), Some(5));
        assert_eq!(query().best(&records[..2]).map(|r| r.id), Some(2));
        assert_eq!(query().best(&records[2..4]), None);
    }
}
