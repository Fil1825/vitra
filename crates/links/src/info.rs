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
    artists: Option<Vec<Option<String>>>,
    creator: Option<String>,
    creators: Option<Vec<Option<String>>>,
    uploader: Option<String>,
    channel: Option<String>,
    album: Option<String>,
    album_artist: Option<String>,
    album_artists: Option<Vec<Option<String>>>,
    track_number: Option<i64>,
    playlist_index: Option<i64>,
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
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkTrack {
    /// What to play (and store as the track's path).
    pub url: String,
    /// The extractor's id, used to match a track with its later, complete metadata.
    pub id: Option<String>,
    pub title: String,
    /// The credit as one text ("A, B" when the site lists several artists).
    pub artist: Option<String>,
    /// The artists as the site lists them; empty when it only gives one text, which the
    /// library then splits (see `library::ArtistSplitting`).
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u16>,
    /// Seconds.
    pub duration: Option<f64>,
    pub live: bool,
    /// The site, e.g. "Youtube" or "Soundcloud".
    pub source: Option<String>,
}

/// What makes a playlist an album.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinkAlbum {
    pub title: String,
    pub artist: Option<String>,
    /// Known once a track's own page was read (listings don't carry it).
    pub year: Option<u16>,
    /// Tracks on the album, unavailable ones included (the "of" in "3 of 12").
    pub count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkPlaylist {
    pub url: String,
    pub id: Option<String>,
    pub title: String,
    pub uploader: Option<String>,
    pub tracks: Vec<LinkTrack>,
    /// Set when the playlist is an album (YouTube Music's album playlists, Bandcamp albums):
    /// its entries are then numbered tracks of that album.
    pub album: Option<LinkAlbum>,
}

/// YouTube's album playlists ("OLAK5uy_…"); YouTube Music's album pages
/// (`browse/MPREb…`) are resolved by yt-dlp to them.
const ALBUM_PLAYLIST_PREFIX: &str = "OLAK5uy_";
/// What YouTube puts before an album playlist's title.
const ALBUM_TITLE_PREFIX: &str = "Album - ";

/// Whether a link is a YouTube or YouTube Music album, judging by the URL alone.
pub fn is_album_link(url: &str) -> bool {
    url.contains(&format!("list={ALBUM_PLAYLIST_PREFIX}")) || url.contains("/browse/MPREb")
}

/// The album a playlist is, if it is one.
fn album_of(raw: &RawInfo, requested: &str, url: &str) -> Option<LinkAlbum> {
    let by_id = raw
        .id
        .as_deref()
        .is_some_and(|id| id.starts_with(ALBUM_PLAYLIST_PREFIX));
    let by_site = raw.extractor_key.as_deref() == Some("BandcampAlbum");
    if !(by_id || by_site || is_album_link(requested) || is_album_link(url)) {
        return None;
    }
    let title = non_empty(raw.title.clone()).unwrap_or_default();
    let title = title
        .strip_prefix(ALBUM_TITLE_PREFIX)
        .unwrap_or(&title)
        .trim()
        .to_owned();
    Some(LinkAlbum {
        title,
        artist: first_of(raw.album_artists.clone())
            .or_else(|| non_empty(raw.album_artist.clone()))
            .or_else(|| without_topic(non_empty(raw.uploader.clone())))
            .or_else(|| without_topic(non_empty(raw.channel.clone()))),
        year: raw.release_year.and_then(|year| u16::try_from(year).ok()),
        count: None,
    })
}

/// YouTube's auto-generated music channels are "Artist - Topic".
fn without_topic(name: Option<String>) -> Option<String> {
    name.map(|name| name.strip_suffix(" - Topic").unwrap_or(&name).to_owned())
}

/// Non-empty, trimmed names of a list.
fn names(list: Option<Vec<Option<String>>>) -> Vec<String> {
    list.unwrap_or_default()
        .into_iter()
        .filter_map(non_empty)
        .collect()
}

fn first_of(list: Option<Vec<Option<String>>>) -> Option<String> {
    names(list).into_iter().next()
}

/// The artist most of an album's tracks are by (their main artist), for albums whose
/// playlist doesn't name one.
fn usual_artist(tracks: &[LinkTrack]) -> Option<String> {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    for track in tracks {
        let Some(artist) = track.artists.first().or(track.artist.as_ref()) else {
            continue;
        };
        match counts.iter_mut().find(|(name, _)| name == artist) {
            Some((_, count)) => *count += 1,
            None => counts.push((artist, 1)),
        }
    }
    // The first of the most frequent, so ties go to the album's opening track.
    let most = counts.iter().map(|(_, count)| *count).max()?;
    counts
        .iter()
        .find(|(_, count)| *count == most)
        .map(|(name, _)| (*name).to_owned())
}

/// Files a track under its album: the album's title and artist, numbered by its place.
pub fn place_in_album(track: &mut LinkTrack, album: &LinkAlbum, number: u32) {
    track.album = Some(album.title.clone());
    track.album_artist.clone_from(&album.artist);
    track.track_number = Some(number);
    if track.year.is_none() {
        track.year = album.year;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    Track(LinkTrack),
    Playlist(LinkPlaylist),
}

/// Parses `yt-dlp -J` output for `requested` (the link the user gave).
pub fn parse_info(json: &str, requested: &str) -> Result<Resolved> {
    let mut raw: RawInfo = serde_json::from_str(json)?;
    if raw.kind.as_deref() == Some("playlist") || raw.entries.is_some() {
        let url = raw
            .webpage_url
            .clone()
            .or_else(|| raw.original_url.clone())
            .unwrap_or_else(|| requested.to_owned());
        let mut album = album_of(&raw, requested, &url);
        let entries = raw.entries.take().unwrap_or_default();
        if let Some(album) = album.as_mut() {
            album.count = u32::try_from(entries.len()).ok().filter(|n| *n > 0);
        }
        let mut tracks: Vec<LinkTrack> = Vec::new();
        let mut numbers: Vec<u32> = Vec::new();
        for (index, entry) in entries.into_iter().enumerate() {
            let Some(entry) = entry else {
                continue;
            };
            // Numbered by place in the listing, unavailable entries included.
            let number = entry
                .playlist_index
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(index as u32 + 1);
            if let Some(track) = track(entry, Some(&url)) {
                tracks.push(track);
                numbers.push(number);
            }
        }
        if let Some(album) = album.as_mut() {
            if album.artist.is_none() {
                album.artist = usual_artist(&tracks);
            }
            for (track, number) in tracks.iter_mut().zip(numbers) {
                place_in_album(track, album, number);
            }
        }
        return Ok(Resolved::Playlist(LinkPlaylist {
            title: non_empty(raw.title).unwrap_or_else(|| url.clone()),
            uploader: non_empty(raw.uploader).or(non_empty(raw.channel)),
            id: raw.id,
            url,
            tracks,
            album,
        }));
    }
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

    // The site's list of artists (YouTube Music, SoundCloud) is what the library stores;
    // a lone text is split by the library.
    let mut listed = names(raw.artists.clone());
    if listed.is_empty() {
        listed = names(raw.creators.clone());
    }
    let tagged_artist = (!listed.is_empty())
        .then(|| listed.join(", "))
        .or_else(|| non_empty(raw.artist.clone()))
        .or_else(|| non_empty(raw.creator.clone()))
        .or_else(|| non_empty(raw.album_artist.clone()));
    let track_title = non_empty(raw.track.clone());
    let uploader =
        without_topic(non_empty(raw.uploader.clone()).or_else(|| non_empty(raw.channel.clone())));

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
    // Only the site's list is kept as a list; an artist taken from the title or uploader is
    // a credit like any other.
    let artists = if artist.as_deref() == Some(listed.join(", ").as_str()) {
        listed
    } else {
        Vec::new()
    };
    Some(LinkTrack {
        url,
        id: raw.id,
        title,
        artist,
        artists,
        album: non_empty(raw.album),
        album_artist: first_of(raw.album_artists).or_else(|| non_empty(raw.album_artist)),
        track_number: raw.track_number.and_then(|n| u32::try_from(n).ok()),
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
        playlist_from(&fixture(name), "https://example.com/list")
    }

    fn playlist_from(json: &str, requested: &str) -> LinkPlaylist {
        match parse_info(json, requested).unwrap() {
            Resolved::Playlist(playlist) => playlist,
            other => panic!("expected a playlist, got {other:?}"),
        }
    }

    /// A fixture that `scripts/record-ytdlp-fixtures.sh` records, if it has been run.
    fn recorded(name: &str) -> Option<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(path).ok()
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
    fn album_links_are_recognized_by_url() {
        assert!(is_album_link(
            "https://music.youtube.com/playlist?list=OLAK5uy_abc"
        ));
        assert!(is_album_link(
            "https://www.youtube.com/playlist?list=OLAK5uy_abc&si=x"
        ));
        assert!(is_album_link(
            "https://music.youtube.com/browse/MPREb_abcdef"
        ));
        assert!(!is_album_link(
            "https://www.youtube.com/playlist?list=PLabc"
        ));
        assert!(!is_album_link("https://www.youtube.com/watch?v=x"));
    }

    #[test]
    fn a_youtube_album_is_an_album_with_numbered_tracks() {
        check_album(&playlist("youtube-album.json"));
        if let Some(json) = recorded("youtube-album-recorded.json") {
            check_album(&playlist_from(&json, "https://example.com/list"));
        }
        if let Some(json) = recorded("youtube-album-page-recorded.json") {
            check_album(&playlist_from(
                &json,
                "https://music.youtube.com/browse/MPREb_recorded",
            ));
        }
    }

    fn check_album(list: &LinkPlaylist) {
        let album = list.album.as_ref().expect("an album");
        assert!(!album.title.is_empty());
        assert!(!album.title.starts_with("Album - "), "{}", album.title);
        let artist = album.artist.as_deref().expect("an album artist");
        assert!(!artist.ends_with(" - Topic"), "{artist}");
        assert!(!list.tracks.is_empty());
        let mut last = 0;
        for track in &list.tracks {
            assert_eq!(track.album.as_deref(), Some(album.title.as_str()));
            assert_eq!(track.album_artist.as_deref(), Some(artist));
            let number = track.track_number.expect("numbered");
            assert!(number > last, "numbers keep the album's order");
            last = number;
            assert!(track.duration.is_some_and(|d| d > 0.0), "{}", track.title);
        }
    }

    #[test]
    fn album_details_of_the_hand_written_fixture() {
        let list = playlist("youtube-album.json");
        let album = list.album.unwrap();
        assert_eq!(album.title, "Polar Lights");
        // The playlist has no owner; the tracks' artist names it.
        assert_eq!(album.artist.as_deref(), Some("Aurora Collective"));
        let numbers: Vec<_> = list.tracks.iter().map(|t| t.track_number).collect();
        // The private video keeps its place, and counts.
        assert_eq!(numbers, [Some(1), Some(2), Some(4)]);
        assert_eq!(album.count, Some(4));
    }

    #[test]
    fn an_album_page_is_an_album_whatever_yt_dlp_calls_it() {
        let json = r#"{"_type":"playlist","id":"VLsomething","title":"Some Album",
            "uploader":"Band - Topic","entries":[{"_type":"url","id":"a",
            "url":"https://www.youtube.com/watch?v=a","title":"One","duration":10}]}"#;
        let Resolved::Playlist(list) =
            parse_info(json, "https://music.youtube.com/browse/MPREb_abc").unwrap()
        else {
            panic!("a playlist");
        };
        let album = list.album.unwrap();
        assert_eq!(
            (album.title.as_str(), album.artist.as_deref()),
            ("Some Album", Some("Band"))
        );
        // An ordinary playlist isn't.
        assert_eq!(playlist("youtube-playlist.json").album, None);
    }

    /// Recordings of real pages (scripts/record-ytdlp-fixtures.sh), checked for what doesn't
    /// depend on which video or set was recorded.
    #[test]
    fn recorded_pages_parse_like_the_hand_written_ones() {
        let track_of = |name: &str, requested: &str| {
            let json = recorded(name)?;
            match parse_info(&json, requested).unwrap() {
                Resolved::Track(track) => Some(track),
                other => panic!("{name}: expected a track, got {other:?}"),
            }
        };
        if let Some(video) = track_of("youtube-video-recorded.json", "https://youtu.be/x") {
            assert!(
                video.url.starts_with("https://www.youtube.com/watch?v="),
                "{}",
                video.url
            );
            assert!(!video.title.is_empty());
            assert!(video.artist.is_some(), "{video:?}");
            assert!(video.duration.is_some_and(|d| d > 0.0), "{video:?}");
            assert!(!video.live);
        }
        // A track of an "Artist - Topic" channel comes with its music tags.
        if let Some(music) = track_of(
            "youtube-music-recorded.json",
            "https://music.youtube.com/watch?v=x",
        ) {
            assert!(!music.artists.is_empty(), "{music:?}");
            assert_eq!(music.artist, Some(music.artists.join(", ")));
            assert!(music.album.is_some(), "{music:?}");
            assert!(!music.title.is_empty());
        }
        if let Some(sound) = track_of(
            "soundcloud-track-recorded.json",
            "https://soundcloud.com/a/b",
        ) {
            assert!(!sound.title.is_empty());
            assert!(sound.artist.is_some(), "{sound:?}");
        }
        if let Some(json) = recorded("soundcloud-set-recorded.json") {
            let set = playlist_from(&json, "https://soundcloud.com/a/sets/b");
            assert_eq!(set.album, None, "a set is a playlist");
        }
    }

    #[test]
    fn a_recorded_album_track_lists_its_artists() {
        let Some(json) = recorded("youtube-album-entry-recorded.json") else {
            return;
        };
        let track = parse_entry(&json.replace('\n', " ")).unwrap().unwrap();
        assert!(!track.artists.is_empty(), "{track:?}");
        assert_eq!(track.artist, Some(track.artists.join(", ")));
        assert!(track.album.is_some(), "{track:?}");
    }

    #[test]
    fn an_album_track_page_lists_its_artists() {
        let line = fixture("youtube-album-entry.json").replace('\n', " ");
        let track = parse_entry(&line).unwrap().unwrap();
        assert_eq!(track.artists, ["Aurora Collective", "Glass Harbor"]);
        assert_eq!(
            track.artist.as_deref(),
            Some("Aurora Collective, Glass Harbor")
        );
        assert_eq!(track.album.as_deref(), Some("Polar Lights"));
        assert_eq!(track.year, Some(2021));
    }

    #[test]
    fn several_artists_come_as_a_list_or_a_credit() {
        let parse = |json: &str| match parse_info(json, "https://y/x").unwrap() {
            Resolved::Track(track) => track,
            other => panic!("a track: {other:?}"),
        };
        let listed = parse(
            r#"{"id":"x","title":"Song","track":"Song","artists":["VILLIAN"," madk1d ",null,""]}"#,
        );
        assert_eq!(listed.artists, ["VILLIAN", "madk1d"]);
        assert_eq!(listed.artist.as_deref(), Some("VILLIAN, madk1d"));
        let creators = parse(r#"{"id":"x","title":"Song","creators":["A","B"]}"#);
        assert_eq!(creators.artists, ["A", "B"]);
        // From the title: a credit for the library to split.
        let credit = parse(r#"{"id":"x","title":"VILLIAN, madk1d - Song","uploader":"Label"}"#);
        assert!(credit.artists.is_empty());
        assert_eq!(credit.artist.as_deref(), Some("VILLIAN, madk1d"));
        assert_eq!(credit.title, "Song");
    }

    #[test]
    fn enrichment_lines_parse_on_their_own() {
        let line = fixture("youtube-video.json").replace('\n', " ");
        let track = parse_entry(&line).unwrap().unwrap();
        assert_eq!(track.id.as_deref(), Some("dQw4w9WgXcQ"));
        assert!(parse_entry("not json").is_err());
    }
}
