use url::Url;

use crate::{LinkError, Result};

/// How a link is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// A web page (YouTube, SoundCloud, ...): yt-dlp extracts the audio.
    Page,
    /// A direct audio file: mpv plays it as is.
    Audio,
    /// A stream or playlist file (internet radio: `.m3u`, `.pls`, HLS): mpv plays it.
    Stream,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub url: String,
    pub kind: LinkKind,
}

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wv", "ape", "mka",
];
const STREAM_EXTENSIONS: &[&str] = &["m3u", "m3u8", "pls", "xspf"];

/// Parses what the user pasted. Only `http(s)` links are accepted (a scheme is added when
/// missing, as in `youtube.com/watch?v=…`), so a pasted link can't make mpv or yt-dlp open
/// local files or exotic protocols.
pub fn parse_link(input: &str) -> Result<Link> {
    let trimmed = input.trim();
    let invalid = || LinkError::InvalidUrl(trimmed.chars().take(200).collect());
    if trimmed.is_empty() || trimmed.chars().any(char::is_whitespace) {
        return Err(invalid());
    }
    let parsed = match Url::parse(trimmed) {
        Ok(url) => url,
        // "example.com/x" has no scheme; anything with a dot before the first slash is a host.
        Err(url::ParseError::RelativeUrlWithoutBase)
            if trimmed
                .split('/')
                .next()
                .is_some_and(|host| host.contains('.')) =>
        {
            Url::parse(&format!("https://{trimmed}")).map_err(|_| invalid())?
        }
        Err(_) => return Err(invalid()),
    };
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(invalid());
    }
    let extension = parsed
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, ext)| ext.to_ascii_lowercase());
    let kind = match extension.as_deref() {
        Some(ext) if AUDIO_EXTENSIONS.contains(&ext) => LinkKind::Audio,
        Some(ext) if STREAM_EXTENSIONS.contains(&ext) => LinkKind::Stream,
        _ => LinkKind::Page,
    };
    Ok(Link {
        url: parsed.into(),
        kind,
    })
}

/// Whether a library path is a link rather than a local file.
pub fn is_remote(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(input: &str) -> LinkKind {
        parse_link(input).unwrap().kind
    }

    #[test]
    fn pages_audio_files_and_streams() {
        assert_eq!(
            kind("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
            LinkKind::Page
        );
        assert_eq!(
            kind("https://soundcloud.com/forss/flickermood"),
            LinkKind::Page
        );
        assert_eq!(
            kind("https://example.com/music/song.MP3?token=1"),
            LinkKind::Audio
        );
        assert_eq!(
            kind("http://radio.example.com:8000/live.pls"),
            LinkKind::Stream
        );
        assert_eq!(
            kind("https://cdn.example.com/hls/index.m3u8"),
            LinkKind::Stream
        );
        // An icecast mount without an extension is a page as far as we can tell; yt-dlp's
        // generic extractor recognizes it as direct audio.
        assert_eq!(kind("http://ice.example.com:8000/stream"), LinkKind::Page);
    }

    #[test]
    fn adds_a_missing_scheme() {
        let link = parse_link("  youtube.com/watch?v=abc ").unwrap();
        assert_eq!(link.url, "https://youtube.com/watch?v=abc");
        assert_eq!(link.kind, LinkKind::Page);
    }

    #[test]
    fn refuses_other_schemes_and_junk() {
        for input in [
            "",
            "file:///etc/passwd",
            "ytdl://something",
            "javascript:alert(1)",
            "just some words",
            "music",
            "ftp://example.com/a.mp3",
        ] {
            assert!(parse_link(input).is_err(), "{input}");
        }
    }

    #[test]
    fn tells_links_from_files() {
        assert!(is_remote("https://youtu.be/x"));
        assert!(!is_remote("/Users/me/Music/a.flac"));
        assert!(!is_remote("C:\\Music\\a.flac"));
    }
}
