//! Reading the track lists that music services put in their public pages and APIs, with no
//! account: Spotify's embed pages, Apple Music's pages, Deezer's public API and Yandex
//! Music's web handlers. Only Deezer's API is meant for others; the rest can change any day,
//! so each reader looks for the shape of a track wherever it is in the page's data rather
//! than following one exact path, and says plainly when it found nothing.

use serde_json::{Map, Value};

use crate::links::{Service, ServiceLink, Target};
use crate::model::{ImportedPlaylist, ImportedTrack};

/// Why a page gave no tracks.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PageError {
    #[error("the service has no such list")]
    NotFound,
    /// Private, or the user's own (Liked Songs, «Мне нравится»).
    #[error("the list is private")]
    Private,
    #[error("the page can't be read right now: {0}")]
    Unreadable(String),
}

/// What to fetch for a link, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub url: String,
    /// Extra headers ("Name: value").
    pub headers: Vec<String>,
}

/// The page or API call that has the tracks of `link`; `None` for links read another way
/// (short links are followed first, VK pages need the user's copy, YouTube & co. play).
pub fn request(link: &ServiceLink) -> Option<Request> {
    let plain = |url: String| Request {
        url,
        headers: Vec::new(),
    };
    match (&link.service, &link.target) {
        (Service::Spotify, Target::Playlist { id }) => Some(plain(format!(
            "https://open.spotify.com/embed/playlist/{id}"
        ))),
        (Service::Spotify, Target::Album { id }) => {
            Some(plain(format!("https://open.spotify.com/embed/album/{id}")))
        }
        (Service::Spotify, Target::Track { id }) => {
            Some(plain(format!("https://open.spotify.com/embed/track/{id}")))
        }
        (
            Service::AppleMusic,
            Target::Playlist { .. } | Target::Album { .. } | Target::Track { .. },
        ) => Some(plain(link.url.clone())),
        (Service::Deezer, Target::Playlist { id }) => {
            Some(plain(format!("https://api.deezer.com/playlist/{id}")))
        }
        (Service::Deezer, Target::Album { id }) => {
            Some(plain(format!("https://api.deezer.com/album/{id}")))
        }
        (Service::Deezer, Target::Track { id }) => {
            Some(plain(format!("https://api.deezer.com/track/{id}")))
        }
        (Service::YandexMusic, target) => yandex_request(&link.url, target),
        _ => None,
    }
}

/// Reads what [`request`] fetched for `link`.
pub fn read(link: &ServiceLink, body: &str) -> Result<ImportedPlaylist, PageError> {
    let playlist = match link.service {
        Service::Spotify => spotify(body)?,
        Service::AppleMusic => apple(body)?,
        Service::Deezer => deezer(body)?.playlist,
        Service::YandexMusic => yandex(body)?,
        _ => return Err(PageError::Unreadable("not a list Vitra reads".into())),
    };
    if playlist.tracks.is_empty() {
        return Err(PageError::Unreadable("no tracks on the page".into()));
    }
    Ok(playlist)
}

/// The JSON inside `<script … id="<id>" …>…</script>`.
pub fn script_json(html: &str, id: &str) -> Option<Value> {
    let mut rest = html;
    loop {
        let at = rest.find("<script")?;
        rest = &rest[at..];
        let close = rest.find('>')?;
        let tag = &rest[..close];
        let body_start = close + 1;
        let end = rest[body_start..].find("</script>")? + body_start;
        if tag.contains(&format!("id=\"{id}\"")) || tag.contains(&format!("id={id}")) {
            return serde_json::from_str(rest[body_start..end].trim()).ok();
        }
        rest = &rest[end..];
    }
}

/// `<meta property="og:title" content="…">`.
fn og_title(html: &str) -> Option<String> {
    let at = html.find("property=\"og:title\"")?;
    let tag_start = html[..at].rfind('<')?;
    let tag_end = at + html[at..].find('>')?;
    let tag = &html[tag_start..tag_end];
    let content = tag.split("content=\"").nth(1)?.split('"').next()?;
    Some(unescape_html(content)).filter(|t| !t.is_empty())
}

fn unescape_html(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Every object in a JSON tree, depth first, in document order.
fn objects<'a>(value: &'a Value, out: &mut Vec<&'a Map<String, Value>>) {
    match value {
        Value::Object(map) => {
            out.push(map);
            for child in map.values() {
                objects(child, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                objects(item, out);
            }
        }
        _ => {}
    }
}

fn text(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn number(map: &Map<String, Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(Value::as_f64).filter(|n| *n > 0.0)
}

fn names(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|artist| artist.as_object().and_then(|a| text(a, "name")))
        .collect()
}

// --- Spotify ------------------------------------------------------------------------

/// Spotify's embed page (`open.spotify.com/embed/…`): its `__NEXT_DATA__` holds the list
/// (`trackList`: title, subtitle = the artists, duration in ms, uri) or the one track.
pub fn spotify(html: &str) -> Result<ImportedPlaylist, PageError> {
    let data = script_json(html, "__NEXT_DATA__")
        .ok_or_else(|| PageError::Unreadable("no data in Spotify's page".into()))?;
    let mut all = Vec::new();
    objects(&data, &mut all);
    if let Some(entity) = all
        .iter()
        .find(|map| map.get("trackList").is_some_and(Value::is_array))
    {
        let tracks: Vec<ImportedTrack> = entity["trackList"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_object)
            .filter_map(spotify_item)
            .collect();
        let cover = all.iter().find_map(|map| {
            map.get("sources")?
                .as_array()?
                .iter()
                .filter_map(Value::as_object)
                .find_map(|source| text(source, "url"))
        });
        return Ok(ImportedPlaylist {
            name: text(entity, "name").or_else(|| text(entity, "title")),
            // The embed lists a hundred tracks at most.
            partial: tracks.len() >= 100,
            tracks,
            cover_url: cover,
        });
    }
    // One track.
    let track = all
        .iter()
        .find(|map| {
            text(map, "type").as_deref() == Some("track")
                && (map.contains_key("name") || map.contains_key("title"))
        })
        .and_then(|map| spotify_item(map))
        .ok_or_else(|| PageError::Unreadable("no tracks in Spotify's page".into()))?;
    Ok(ImportedPlaylist {
        tracks: vec![track],
        ..ImportedPlaylist::default()
    })
}

fn spotify_item(item: &Map<String, Value>) -> Option<ImportedTrack> {
    let title = text(item, "title").or_else(|| text(item, "name"))?;
    let mut artists = names(item.get("artists"));
    if artists.is_empty()
        && let Some(subtitle) = text(item, "subtitle")
    {
        // Joined with ", ", the space often a no-break one.
        artists = subtitle
            .replace('\u{a0}', " ")
            .split(", ")
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect();
    }
    let url = text(item, "uri").and_then(|uri| {
        uri.strip_prefix("spotify:track:")
            .map(|id| format!("https://open.spotify.com/track/{id}"))
    });
    Some(ImportedTrack {
        duration: number(item, "duration").map(|ms| ms / 1000.0),
        url,
        ..ImportedTrack::new(title, artists)
    })
}

// --- Apple Music --------------------------------------------------------------------

/// An Apple Music page: its `serialized-server-data` has the track lockups (title,
/// artistName, duration in ms); its schema.org data is the fallback.
pub fn apple(html: &str) -> Result<ImportedPlaylist, PageError> {
    let name = og_title(html).map(|title| {
        title
            .trim_end_matches(" on Apple Music")
            .trim_end_matches(" - Apple Music")
            .trim_end_matches(" — Apple Music")
            .to_owned()
    });
    let mut tracks = Vec::new();
    if let Some(data) = script_json(html, "serialized-server-data") {
        let mut all = Vec::new();
        objects(&data, &mut all);
        // The track section, if it says it is one; else every track-shaped object.
        let section = all.iter().find(|map| {
            text(map, "itemKind").as_deref() == Some("trackLockup")
                && map.get("items").is_some_and(Value::is_array)
        });
        let candidates: Vec<&Map<String, Value>> = match section {
            Some(section) => section["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
                .collect(),
            None => all,
        };
        tracks = candidates
            .into_iter()
            .filter_map(|item| {
                let title = text(item, "title")?;
                let artist = text(item, "artistName")?;
                let duration = number(item, "duration").map(|ms| ms / 1000.0);
                let album = text(item, "albumName");
                Some(ImportedTrack {
                    album,
                    duration,
                    ..ImportedTrack::new(title, vec![artist])
                })
            })
            .collect();
    }
    if tracks.is_empty() {
        tracks = apple_schema(html);
    }
    Ok(ImportedPlaylist {
        name,
        tracks,
        ..ImportedPlaylist::default()
    })
}

/// schema.org `MusicRecording`s in the page's JSON-LD.
fn apple_schema(html: &str) -> Vec<ImportedTrack> {
    let mut tracks = Vec::new();
    for id in ["schema:music-playlist", "schema:music-album", "schema:song"] {
        let Some(data) = script_json(html, id) else {
            continue;
        };
        let mut all = Vec::new();
        objects(&data, &mut all);
        let album_artists = all
            .first()
            .map(|root| names(root.get("byArtist")))
            .unwrap_or_default();
        for map in all {
            if text(map, "@type").as_deref() != Some("MusicRecording") {
                continue;
            }
            let Some(title) = text(map, "name") else {
                continue;
            };
            let mut artists = names(map.get("byArtist"));
            if artists.is_empty() {
                artists.clone_from(&album_artists);
            }
            tracks.push(ImportedTrack {
                duration: text(map, "duration").and_then(|d| iso_duration(&d)),
                url: text(map, "url"),
                ..ImportedTrack::new(title, artists)
            });
        }
    }
    tracks
}

/// "PT3M45S" in seconds.
fn iso_duration(text: &str) -> Option<f64> {
    let rest = text.strip_prefix("PT")?;
    let mut seconds = 0.0;
    let mut number = String::new();
    for c in rest.chars() {
        match c {
            '0'..='9' | '.' => number.push(c),
            'H' | 'M' | 'S' => {
                let value: f64 = number.parse().ok()?;
                number.clear();
                seconds += value
                    * match c {
                        'H' => 3600.0,
                        'M' => 60.0,
                        _ => 1.0,
                    };
            }
            _ => return None,
        }
    }
    (seconds > 0.0).then_some(seconds)
}

// --- Deezer -------------------------------------------------------------------------

/// A page of Deezer's API, and the link to the next one.
#[derive(Debug, Clone, PartialEq)]
pub struct DeezerPage {
    pub playlist: ImportedPlaylist,
    /// Long playlists come in pages.
    pub next: Option<String>,
}

/// Deezer's public API: a playlist or album (`title`, `tracks.data`), a page of its tracks
/// (`data`, `next`), or one track.
pub fn deezer(json: &str) -> Result<DeezerPage, PageError> {
    let value: Value =
        serde_json::from_str(json).map_err(|err| PageError::Unreadable(err.to_string()))?;
    let root = value
        .as_object()
        .ok_or_else(|| PageError::Unreadable("not an object".into()))?;
    if let Some(error) = root.get("error").and_then(Value::as_object) {
        let code = error.get("code").and_then(Value::as_i64);
        return Err(match code {
            Some(800) => PageError::NotFound,
            _ => PageError::Unreadable(text(error, "message").unwrap_or_default()),
        });
    }
    let track = |item: &Map<String, Value>| -> Option<ImportedTrack> {
        let title = text(item, "title")?;
        let artist = item
            .get("artist")
            .and_then(Value::as_object)
            .and_then(|a| text(a, "name"));
        Some(ImportedTrack {
            album: item
                .get("album")
                .and_then(Value::as_object)
                .and_then(|a| text(a, "title")),
            duration: number(item, "duration"),
            isrc: text(item, "isrc"),
            url: text(item, "link"),
            ..ImportedTrack::new(title, artist.into_iter().collect())
        })
    };
    // A single track.
    if text(root, "type").as_deref() == Some("track") {
        return Ok(DeezerPage {
            playlist: ImportedPlaylist {
                tracks: track(root).into_iter().collect(),
                ..ImportedPlaylist::default()
            },
            next: None,
        });
    }
    let list = root
        .get("tracks")
        .and_then(Value::as_object)
        .unwrap_or(root);
    let tracks = list
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(track)
        .collect();
    let cover = text(root, "picture_xl").or_else(|| text(root, "cover_xl"));
    Ok(DeezerPage {
        playlist: ImportedPlaylist {
            name: text(root, "title"),
            tracks,
            cover_url: cover,
            ..ImportedPlaylist::default()
        },
        next: text(list, "next"),
    })
}

// --- Yandex Music -------------------------------------------------------------------

/// The web handlers Yandex Music's own pages use (yt-dlp reads the same), on the link's
/// own domain; the new `/playlists/<uuid>` share links go through its API.
fn yandex_request(url: &str, target: &Target) -> Option<Request> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    let origin = format!("https://{host}");
    let handler = match target {
        Target::Album { id } => format!("{origin}/handlers/album.jsx?album={id}"),
        Target::Track { id } => format!("{origin}/handlers/track.jsx?track={id}"),
        Target::UserPlaylist { owner, id } => format!(
            "{origin}/handlers/playlist.jsx?owner={}&kinds={}&light=false&lang=ru&external-domain={host}&overembed=false",
            encode(owner),
            encode(id)
        ),
        Target::Playlist { id } => {
            return Some(Request {
                url: format!(
                    "https://api.music.yandex.net/playlist/{}?richTracks=true",
                    encode(id)
                ),
                headers: Vec::new(),
            });
        }
        _ => return None,
    };
    Some(Request {
        url: handler,
        headers: vec![
            "X-Requested-With: XMLHttpRequest".into(),
            format!("X-Retpath-Y: {url}"),
            format!("Referer: {url}"),
        ],
    })
}

fn encode(text: &str) -> String {
    url::form_urlencoded::byte_serialize(text.as_bytes()).collect()
}

/// Yandex Music's JSON: tracks are objects with a `title`, `artists` and `durationMs`,
/// wherever they sit (an album's volumes, a playlist's tracks, the API's `result`).
pub fn yandex(json: &str) -> Result<ImportedPlaylist, PageError> {
    let value: Value =
        serde_json::from_str(json).map_err(|err| PageError::Unreadable(err.to_string()))?;
    let mut all = Vec::new();
    objects(&value, &mut all);
    // A closed list answers with an error or without its tracks.
    if let Some(root) = value.as_object() {
        if root.get("error").is_some() || text(root, "result").as_deref() == Some("fail") {
            return Err(PageError::NotFound);
        }
        let playlist = root
            .get("playlist")
            .or_else(|| root.get("result"))
            .and_then(Value::as_object);
        if let Some(playlist) = playlist
            && text(playlist, "visibility").as_deref() == Some("private")
            && playlist.get("tracks").is_none()
        {
            return Err(PageError::Private);
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut tracks = Vec::new();
    for map in &all {
        let Some(duration) = number(map, "durationMs") else {
            continue;
        };
        let Some(mut title) = text(map, "title") else {
            continue;
        };
        if !map.get("artists").is_some_and(Value::is_array) {
            continue;
        }
        // The same track object can appear twice (in `tracks` and a nested `track`).
        let id = map.get("id").map(Value::to_string).unwrap_or_default();
        if !id.is_empty() && !seen.insert(id) {
            continue;
        }
        if let Some(version) = text(map, "version") {
            title = format!("{title} ({version})");
        }
        let album = map
            .get("albums")
            .and_then(Value::as_array)
            .and_then(|albums| albums.first())
            .and_then(Value::as_object)
            .and_then(|a| text(a, "title"));
        tracks.push(ImportedTrack {
            album,
            duration: Some(duration / 1000.0),
            ..ImportedTrack::new(title, names(map.get("artists")))
        });
    }
    let root = value.as_object();
    let list = root.and_then(|root| {
        root.get("playlist")
            .or_else(|| root.get("result"))
            .and_then(Value::as_object)
    });
    let name = list
        .and_then(|l| text(l, "title"))
        .or_else(|| root.and_then(|r| text(r, "title")));
    let declared = list
        .and_then(|l| l.get("trackIds").or_else(|| l.get("trackCount")))
        .and_then(|v| {
            v.as_array()
                .map(Vec::len)
                .or_else(|| v.as_u64().map(|n| n as usize))
        });
    Ok(ImportedPlaylist {
        name,
        partial: declared.is_some_and(|n| n > tracks.len()),
        tracks,
        ..ImportedPlaylist::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(url: &str) -> ServiceLink {
        crate::links::parse(url).unwrap()
    }

    #[test]
    fn requests() {
        assert_eq!(
            request(&link("https://open.spotify.com/playlist/abc?si=1"))
                .unwrap()
                .url,
            "https://open.spotify.com/embed/playlist/abc"
        );
        assert_eq!(
            request(&link("https://www.deezer.com/fr/album/302127"))
                .unwrap()
                .url,
            "https://api.deezer.com/album/302127"
        );
        let yandex = request(&link("https://music.yandex.ru/users/some-one/playlists/3")).unwrap();
        assert!(
            yandex.url.starts_with(
                "https://music.yandex.ru/handlers/playlist.jsx?owner=some-one&kinds=3"
            )
        );
        assert!(yandex.headers.iter().any(|h| h.starts_with("X-Retpath-Y")));
        assert_eq!(request(&link("https://vk.com/music/playlist/1_2")), None);
    }

    #[test]
    fn finds_scripts_by_id() {
        let html = r#"<html><script>var x = 1;</script><script id="__NEXT_DATA__" type="application/json">{"a":1}</script></html>"#;
        assert_eq!(
            script_json(html, "__NEXT_DATA__"),
            Some(serde_json::json!({"a": 1}))
        );
        assert_eq!(script_json(html, "nope"), None);
    }

    #[test]
    fn iso_durations() {
        assert_eq!(iso_duration("PT3M45S"), Some(225.0));
        assert_eq!(iso_duration("PT1H2M3S"), Some(3723.0));
        assert_eq!(iso_duration("3:45"), None);
    }
}
