//! Links to playlists, albums and tracks on music services, and what pasted text with links
//! in it amounts to.

use serde::Serialize;
use url::Url;

/// A music service Vitra reads lists from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Service {
    Spotify,
    AppleMusic,
    Deezer,
    YandexMusic,
    Vk,
    /// YouTube, YouTube Music, SoundCloud, Bandcamp: playable as they are (yt-dlp).
    Playable,
}

/// What a link shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Target {
    Playlist {
        id: String,
    },
    Album {
        id: String,
    },
    Track {
        id: String,
    },
    /// Yandex Music's old-style playlists: `/users/<owner>/playlists/<kind>`.
    UserPlaylist {
        owner: String,
        id: String,
    },
    /// A short link (`spotify.link`, `deezer.page.link`) that must be followed first.
    Short,
    /// Something on the service that isn't a list (an artist page, the home page).
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceLink {
    pub service: Service,
    pub target: Target,
    /// The link as given (cleaned of tracking parameters).
    pub url: String,
}

/// What `input` links to, if it is a link to a music service.
pub fn parse(input: &str) -> Option<ServiceLink> {
    let input = input.trim();
    if let Some(rest) = input.strip_prefix("spotify:") {
        let (kind, id) = rest.split_once(':')?;
        let target = target(kind, id)?;
        return Some(ServiceLink {
            service: Service::Spotify,
            url: format!("https://open.spotify.com/{kind}/{id}"),
            target,
        });
    }
    let url = Url::parse(input).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let host = url
        .host_str()?
        .trim_start_matches("www.")
        .to_ascii_lowercase();
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    let (service, target) = match host.as_str() {
        "open.spotify.com" | "play.spotify.com" => {
            // `/intl-de/track/…` and `/embed/playlist/…` too.
            let path: Vec<&str> = segments
                .iter()
                .copied()
                .filter(|s| !s.starts_with("intl-") && *s != "embed")
                .collect();
            let target = match path.as_slice() {
                [kind, id, ..] => target(kind, id).unwrap_or(Target::Other),
                _ => Target::Other,
            };
            (Service::Spotify, target)
        }
        "spotify.link" | "spoti.fi" => (Service::Spotify, Target::Short),
        "music.apple.com" | "itunes.apple.com" => (Service::AppleMusic, apple(&url, &segments)),
        "deezer.com" => {
            // `/en/playlist/…` or `/playlist/…`.
            let path: Vec<&str> = segments
                .iter()
                .copied()
                .skip_while(|s| s.len() == 2)
                .collect();
            let target = match path.as_slice() {
                [kind, id, ..] if id.chars().all(|c| c.is_ascii_digit()) => {
                    target(kind, id).unwrap_or(Target::Other)
                }
                _ => Target::Other,
            };
            (Service::Deezer, target)
        }
        "deezer.page.link" | "link.deezer.com" | "dzr.page.link" => {
            (Service::Deezer, Target::Short)
        }
        host if host.starts_with("music.yandex.") => (Service::YandexMusic, yandex(&segments)),
        "vk.com" | "vk.ru" | "m.vk.com" | "m.vk.ru" => (Service::Vk, Target::Other),
        "youtube.com" | "m.youtube.com" | "music.youtube.com" | "youtu.be" | "soundcloud.com"
        | "on.soundcloud.com" => (Service::Playable, Target::Other),
        host if host.ends_with(".bandcamp.com") => (Service::Playable, Target::Other),
        _ => return None,
    };
    let mut clean = url.clone();
    // Tracking parameters; Apple's `?i=` names the track and stays.
    if matches!(service, Service::Spotify | Service::Deezer) {
        clean.set_query(None);
    }
    clean.set_fragment(None);
    Some(ServiceLink {
        service,
        target,
        url: clean.to_string(),
    })
}

fn target(kind: &str, id: &str) -> Option<Target> {
    let id = id.to_owned();
    match kind {
        "playlist" => Some(Target::Playlist { id }),
        "album" => Some(Target::Album { id }),
        "track" => Some(Target::Track { id }),
        _ => None,
    }
}

/// `music.apple.com/<country>/<kind>/<name>/<id>`; an album link with `?i=` is one song.
fn apple(url: &Url, segments: &[&str]) -> Target {
    let song = url
        .query_pairs()
        .find(|(key, _)| key == "i")
        .map(|(_, value)| value.into_owned());
    let path: Vec<&str> = segments
        .iter()
        .copied()
        .skip_while(|s| s.len() == 2)
        .collect();
    match path.as_slice() {
        ["album", .., id] if song.is_some() => Target::Track {
            id: song.unwrap_or_else(|| (*id).to_owned()),
        },
        ["playlist", .., id] => Target::Playlist {
            id: (*id).to_owned(),
        },
        ["album", .., id] => Target::Album {
            id: (*id).to_owned(),
        },
        ["song", .., id] => Target::Track {
            id: (*id).to_owned(),
        },
        _ => Target::Other,
    }
}

/// `/album/<id>`, `/album/<id>/track/<id>`, `/track/<id>`, `/users/<owner>/playlists/<n>`,
/// and the new share links `/playlists/<uuid>`.
fn yandex(segments: &[&str]) -> Target {
    match segments {
        ["album", _, "track", id, ..] | ["track", id, ..] => Target::Track {
            id: (*id).to_owned(),
        },
        ["album", id, ..] => Target::Album {
            id: (*id).to_owned(),
        },
        ["users", owner, "playlists", id, ..] => Target::UserPlaylist {
            owner: (*owner).to_owned(),
            id: (*id).to_owned(),
        },
        ["playlists", id, ..] => Target::Playlist {
            id: (*id).to_owned(),
        },
        _ => Target::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target_of(url: &str) -> (Service, Target) {
        let link = parse(url).unwrap_or_else(|| panic!("{url}"));
        (link.service, link.target)
    }

    #[test]
    fn spotify() {
        assert_eq!(
            target_of("https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=abc"),
            (
                Service::Spotify,
                Target::Playlist {
                    id: "37i9dQZF1DXcBWIGoYBM5M".into()
                }
            )
        );
        assert_eq!(
            target_of("https://open.spotify.com/intl-de/track/4uLU6hMCjMI75M1A2tKUQC").1,
            Target::Track {
                id: "4uLU6hMCjMI75M1A2tKUQC".into()
            }
        );
        let uri = parse("spotify:album:1DFixLWuPkv3KT3TnV35m3").unwrap();
        assert_eq!(
            uri.url,
            "https://open.spotify.com/album/1DFixLWuPkv3KT3TnV35m3"
        );
        assert_eq!(target_of("https://spotify.link/AbCd").1, Target::Short);
        assert_eq!(
            parse("https://open.spotify.com/playlist/x?si=1")
                .unwrap()
                .url,
            "https://open.spotify.com/playlist/x"
        );
    }

    #[test]
    fn apple_music() {
        assert_eq!(
            target_of("https://music.apple.com/us/playlist/todays-hits/pl.f4d106fed2bd41149aaacabb233eb5eb").1,
            Target::Playlist {
                id: "pl.f4d106fed2bd41149aaacabb233eb5eb".into()
            }
        );
        assert_eq!(
            target_of("https://music.apple.com/ru/album/discovery/697194953").1,
            Target::Album {
                id: "697194953".into()
            }
        );
        assert_eq!(
            target_of("https://music.apple.com/ru/album/discovery/697194953?i=697195462").1,
            Target::Track {
                id: "697195462".into()
            }
        );
    }

    #[test]
    fn deezer_and_yandex() {
        assert_eq!(
            target_of("https://www.deezer.com/en/playlist/908622995").1,
            Target::Playlist {
                id: "908622995".into()
            }
        );
        assert_eq!(
            target_of("https://deezer.page.link/xyz"),
            (Service::Deezer, Target::Short)
        );
        assert_eq!(
            target_of("https://music.yandex.ru/album/3389008/track/28566745").1,
            Target::Track {
                id: "28566745".into()
            }
        );
        assert_eq!(
            target_of("https://music.yandex.ru/users/someone/playlists/3").1,
            Target::UserPlaylist {
                owner: "someone".into(),
                id: "3".into()
            }
        );
        assert_eq!(
            target_of("https://music.yandex.com/playlists/lk.6c1a8c2e-1b9e-4d2d-9a4e-6f1f9e0a2b3c")
                .1,
            Target::Playlist {
                id: "lk.6c1a8c2e-1b9e-4d2d-9a4e-6f1f9e0a2b3c".into()
            }
        );
    }

    #[test]
    fn others() {
        assert_eq!(
            target_of("https://vk.com/music/playlist/-1_2_abc").0,
            Service::Vk
        );
        assert_eq!(
            target_of("https://music.youtube.com/playlist?list=PL123").0,
            Service::Playable
        );
        assert_eq!(
            target_of("https://artist.bandcamp.com/album/x").0,
            Service::Playable
        );
        assert_eq!(parse("https://example.com/list"), None);
        assert_eq!(parse("Daft Punk - One More Time"), None);
    }
}
