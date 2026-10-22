//! What yt-dlp reports about a link (`-J`, `-j`), reduced to what the library stores.

use serde::Deserialize;

use crate::Result;

/// The subset of yt-dlp's info dict Vitra reads. Every field is optional: extractors fill in
/// very different sets.
#[derive(Debug, Default, Deserialize)]
struct RawInfo {
    #[serde(rename = "_type")]
    kind: Option<String>,
    id: Option<String>,
    title: Option<String>,
    track: Option<String>,
    artist: Option<String>,
    artists: Option<Vec<String>>,
    creator: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    release_year: Option<i64>,
    upload_date: Option<String>,
    duration: Option<f64>,
    webpage_url: Option<String>,
    original_url: Option<String>,
    url: Option<String>,
    extractor_key: Option<String>,
    ie_key: Option<String>,
    is_live: Option<bool>,
    live_status: Option<String>,
    entries: Option<Vec<Option<RawInfo>>>,
}

/// One playable item.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkTrack {
    /// What to play (and store as the track's path).
    pub url: String,
    /// The extractor's id, used to match a track with its later, complete metadata.
    pub id: Option<String>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u16>,
    /// Seconds.
    pub duration: Option<f64>,
    pub live: bool,
    /// The site, e.g. "Youtube" or "Soundcloud".
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkPlaylist {
    pub url: String,
    pub id: Option<String>,
    pub title: String,
    pub uploader: Option<String>,
    pub tracks: Vec<LinkTrack>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Track(LinkTrack),
    Playlist(LinkPlaylist),
}

/// Parses `yt-dlp -J` output for `requested` (the link the user gave).
pub fn parse_info(json: &str, requested: &str) -> Result<Resolved> {
    let raw: RawInfo = serde_json::from_str(json)?;
    if raw.kind.as_deref() == Some("playlist") || raw.entries.is_some() {
        let url = raw
            .webpage_url
            .clone()
            .or_else(|| raw.original_url.clone())
            .unwrap_or_else(|| requested.to_owned());
        let tracks = raw
            .entries
            .unwrap_or_default()
            .into_iter()
            .flatten()
            .filter_map(|entry| track(entry, Some(&url)))
            .collect();
        return Ok(Resolved::Playlist(LinkPlaylist {
            title: non_empty(raw.title).unwrap_or_else(|| url.clone()),
            uploader: non_empty(raw.uploader).or(non_empty(raw.channel)),
            id: raw.id,
            url,
            tracks,
        }));
    }
    let mut raw = raw;
    if raw.webpage_url.is_none() && raw.original_url.is_none() {
        raw.webpage_url = Some(requested.to_owned());
    }
    track(raw, None)
        .map(Resolved::Track)
        .ok_or_else(|| crate::LinkError::Failed("the link has nothing to play".into()))
}

/// Parses one `yt-dlp -j` line (complete metadata of one playlist entry).
pub fn parse_entry(json: &str) -> Result<Option<LinkTrack>> {
    let raw: RawInfo = serde_json::from_str(json)?;
    Ok(track(raw, None))
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// Placeholders YouTube shows for entries that can't be played.
fn unavailable(title: &str) -> bool {
    matches!(
        title,
        "[Private video]" | "[Deleted video]" | "[Unavailable video]"
    )
}

/// Noise video titles carry that a music library doesn't want.
const TITLE_NOISE: &[&str] = &[
    "(official video)",
    "(official music video)",
    "(official audio)",
    "(official lyric video)",
    "(official visualizer)",
    "(lyric video)",
    "(lyrics)",
    "(audio)",
    "(visualizer)",
    "[official video]",
    "[official music video]",
    "[official audio]",
    "[lyric video]",
    "[lyrics]",
    "[audio]",
    "(4k remaster)",
    "(hd)",
    "[hd]",
];

fn strip_noise(title: &str) -> String {
    let mut result = title.trim().to_owned();
    loop {
        let lower = result.to_lowercase();
        let Some(noise) = TITLE_NOISE.iter().find(|noise| lower.ends_with(*noise)) else {
            break;
        };
        // `to_lowercase` keeps byte lengths for these ASCII suffixes.
        result.truncate(result.len() - noise.len());
        result = result.trim_end().to_owned();
    }
    result
}

/// Picks the playable URL of an entry. Flat entries (`_type: url`) point at their page; full
/// entries have their own page, unless several media share one page (then the direct media
/// URL is the only thing telling them apart).
fn entry_url(raw: &RawInfo, playlist_url: Option<&str>) -> Option<String> {
    if matches!(raw.kind.as_deref(), Some("url" | "url_transparent")) {
        return raw.url.clone().or_else(|| raw.webpage_url.clone());
    }
    match raw.webpage_url.as_deref() {
        Some(page) if Some(page) != playlist_url => Some(page.to_owned()),
        _ if playlist_url.is_some() => raw.url.clone().or_else(|| raw.webpage_url.clone()),
        _ => raw
            .webpage_url
            .clone()
            .or_else(|| raw.original_url.clone())
            .or_else(|| raw.url.clone()),
    }
}

fn track(raw: RawInfo, playlist_url: Option<&str>) -> Option<LinkTrack> {
    let url = entry_url(&raw, playlist_url)?;
    let raw_title = non_empty(raw.title.clone());
    if raw_title.as_deref().is_some_and(unavailable) {
        return None;
    }

    let tagged_artist = raw
        .artists
        .as_ref()
        .map(|artists| {
            artists
                .iter()
                .map(|a| a.trim())
                .filter(|a| !a.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|joined| !joined.is_empty())
        .or_else(|| non_empty(raw.artist.clone()))
        .or_else(|| non_empty(raw.creator.clone()))
        .or_else(|| non_empty(raw.album_artist.clone()));
    let track_title = non_empty(raw.track.clone());
    let uploader = non_empty(raw.uploader.clone())
        .or_else(|| non_empty(raw.channel.clone()))
        // YouTube's auto-generated music channels are "Artist - Topic".
        .map(|name| name.strip_suffix(" - Topic").unwrap_or(&name).to_owned());

    let (artist, title) = match (tagged_artist, track_title) {
        (artist, Some(title)) => (artist.or(uploader), title),
        (Some(artist), None) => (
            Some(artist),
            strip_noise(raw_title.as_deref().unwrap_or("")),
        ),
        (None, None) => {
            let title = raw_title.as_deref().map(strip_noise).unwrap_or_default();
            // A plain video title is often "Artist - Title"; trust it over the uploader,
            // which is usually a label or a fan channel. Only an unambiguous split counts.
            match title.split_once(" - ") {
                Some((artist, song))
                    if !song.contains(" - ") && !artist.is_empty() && !song.is_empty() =>
                {
                    (Some(artist.trim().to_owned()), song.trim().to_owned())
                }
                _ => (uploader, title),
            }
        }
    };
    let title = if title.is_empty() {
        raw.id
            .clone()
            .unwrap_or_else(|| url.rsplit('/').next().unwrap_or(&url).to_owned())
    } else {
        title
    };

    let year = raw
        .release_year
        .and_then(|year| u16::try_from(year).ok())
        .or_else(|| {
            raw.upload_date
                .as_deref()
                .and_then(|date| date.get(..4))
                .and_then(|year| year.parse().ok())
        });
    Some(LinkTrack {
        url,
        id: raw.id,
        title,
        artist,
        album: non_empty(raw.album),
        year,
        duration: raw.duration.filter(|d| *d > 0.0),
        live: raw.is_live == Some(true) || raw.live_status.as_deref() == Some("is_live"),
        source: raw.extractor_key.or(raw.ie_key),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    fn single(name: &str, requested: &str) -> LinkTrack {
        match parse_info(&fixture(name), requested).unwrap() {
            Resolved::Track(track) => track,
            other => panic!("expected a track, got {other:?}"),
        }
    }

    fn playlist(name: &str) -> LinkPlaylist {
        match parse_info(&fixture(name), "https://example.com/list").unwrap() {
            Resolved::Playlist(playlist) => playlist,
            other => panic!("expected a playlist, got {other:?}"),
        }
    }

    #[test]
    fn a_youtube_video_splits_artist_and_title() {
        let track = single("youtube-video.json", "https://youtu.be/dQw4w9WgXcQ");
        assert_eq!(track.url, "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(track.artist.as_deref(), Some("Rick Astley"));
        assert_eq!(track.title, "Never Gonna Give You Up");
        assert_eq!(track.duration, Some(213.0));
        assert_eq!(track.year, Some(2009));
        assert_eq!(track.source.as_deref(), Some("Youtube"));
        assert!(!track.live);
    }

    #[test]
    fn a_youtube_music_track_uses_its_music_tags() {
        let track = single(
            "youtube-music.json",
            "https://music.youtube.com/watch?v=xyz",
        );
        assert_eq!(track.title, "Midnight City");
        assert_eq!(track.artist.as_deref(), Some("M83"));
        assert_eq!(track.album.as_deref(), Some("Hurry Up, We're Dreaming"));
        assert_eq!(track.year, Some(2011));
    }

    #[test]
    fn a_flat_youtube_playlist_skips_unavailable_videos() {
        let list = playlist("youtube-playlist.json");
        assert_eq!(list.title, "Night Drive");
        assert_eq!(list.uploader.as_deref(), Some("Vitra Tester"));
        let titles: Vec<_> = list.tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Nightcall", "Resonance", "Tarantula"]);
        assert_eq!(list.tracks[0].artist.as_deref(), Some("Kavinsky"));
        // "Artist - Topic" channels lose the suffix.
        assert_eq!(list.tracks[1].artist.as_deref(), Some("HOME"));
        assert_eq!(
            list.tracks[2].url,
            "https://www.youtube.com/watch?v=ccccccccccc"
        );
        assert_eq!(list.tracks[2].id.as_deref(), Some("ccccccccccc"));
    }

    #[test]
    fn a_soundcloud_track_and_set() {
        let track = single(
            "soundcloud-track.json",
            "https://soundcloud.com/forss/flickermood",
        );
        assert_eq!(track.title, "Flickermood");
        assert_eq!(track.artist.as_deref(), Some("Forss"));
        assert_eq!(track.duration, Some(213.951));
        let set = playlist("soundcloud-set.json");
        assert_eq!(set.tracks.len(), 2);
        assert_eq!(
            set.tracks[1].url,
            "https://soundcloud.com/forss/sets/soulhack/track-2"
        );
    }

    #[test]
    fn a_page_with_one_audio_element() {
        // Recorded from yt-dlp's generic extractor against a local page.
        let track = single("generic-song.json", "http://127.0.0.1:8765/song.html");
        assert_eq!(track.url, "http://127.0.0.1:8765/song.html");
        assert_eq!(track.title, "Northern Song (1)");
        assert_eq!(track.source.as_deref(), Some("HTML5MediaEmbed"));
    }

    #[test]
    fn a_page_with_several_audio_elements_plays_each_media_url() {
        // Recorded: both entries share the page URL, so their media URLs tell them apart.
        let list = playlist("generic-album.json");
        let urls: Vec<_> = list.tracks.iter().map(|t| t.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "http://127.0.0.1:8765/single.flac",
                "http://127.0.0.1:8765/wave.flac"
            ]
        );
    }

    #[test]
    fn a_live_stream() {
        let json = r#"{"id":"live1","title":"Lofi Radio 24/7","uploader":"Lofi Girl",
            "webpage_url":"https://www.youtube.com/watch?v=live1","is_live":true,
            "live_status":"is_live"}"#;
        let Resolved::Track(track) = parse_info(json, "https://youtu.be/live1").unwrap() else {
            panic!("a track");
        };
        assert!(track.live);
        assert_eq!(track.duration, None);
        assert_eq!(track.artist.as_deref(), Some("Lofi Girl"));
    }

    #[test]
    fn strips_video_noise_from_titles() {
        assert_eq!(strip_noise("Song (Official Video) [HD]"), "Song");
        assert_eq!(strip_noise("Song (Live)"), "Song (Live)");
        assert_eq!(strip_noise("Песня (Lyrics)"), "Песня");
    }

    #[test]
    fn ambiguous_titles_keep_the_uploader() {
        let json = r#"{"id":"x","title":"Interview - Part 1 - Uncut","uploader":"Radio Show",
            "webpage_url":"https://example.com/x"}"#;
        let Resolved::Track(track) = parse_info(json, "https://example.com/x").unwrap() else {
            panic!("a track");
        };
        assert_eq!(track.artist.as_deref(), Some("Radio Show"));
        assert_eq!(track.title, "Interview - Part 1 - Uncut");
    }

    #[test]
    fn a_page_without_a_url_uses_the_requested_link() {
        let json = r#"{"id":"abc","title":"Something"}"#;
        let Resolved::Track(track) = parse_info(json, "https://example.com/abc").unwrap() else {
            panic!("a track");
        };
        assert_eq!(track.url, "https://example.com/abc");
    }

    #[test]
    fn enrichment_lines_parse_on_their_own() {
        let line = fixture("youtube-video.json").replace('\n', " ");
        let track = parse_entry(&line).unwrap().unwrap();
        assert_eq!(track.id.as_deref(), Some("dQw4w9WgXcQ"));
        assert!(parse_entry("not json").is_err());
    }
}
