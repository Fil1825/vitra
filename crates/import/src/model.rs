//! What an import brings: a list of tracks as another service or file describes them.

use serde::{Deserialize, Serialize};

/// One track of an imported playlist, as its source names it. Only the description comes
/// over; the sound is found later, in the library or on YouTube Music.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportedTrack {
    pub title: String,
    /// Main artist first.
    pub artists: Vec<String>,
    pub album: Option<String>,
    /// Seconds.
    pub duration: Option<f64>,
    pub isrc: Option<String>,
    /// The track's own page on its service (a Spotify or Deezer track link).
    pub url: Option<String>,
    /// A local file the source points at (M3U, XSPF, the Music app).
    pub path: Option<String>,
    /// Read from a line like "A - B", where either side may be the artist.
    pub swappable: bool,
}

impl ImportedTrack {
    pub fn new(title: impl Into<String>, artists: Vec<String>) -> Self {
        Self {
            title: title.into(),
            artists,
            ..Self::default()
        }
    }

    /// The same track with title and artist the other way round (for [`Self::swappable`]).
    pub fn swapped(&self) -> Option<Self> {
        if !self.swappable || self.artists.len() != 1 {
            return None;
        }
        Some(Self {
            title: self.artists[0].clone(),
            artists: vec![self.title.clone()],
            swappable: false,
            ..self.clone()
        })
    }

    /// "Artist — Title", for searching and showing.
    pub fn display(&self) -> String {
        match self.artists.first() {
            Some(artist) => format!("{artist} — {}", self.title),
            None => self.title.clone(),
        }
    }
}

/// A playlist read from somewhere.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImportedPlaylist {
    pub name: Option<String>,
    pub tracks: Vec<ImportedTrack>,
    /// Its cover on the service, when it has one.
    pub cover_url: Option<String>,
    /// The service gave only the first part of a longer list (Spotify's embed page).
    pub partial: bool,
}

/// Seconds from "3:45", "1:02:03" or a plain number of seconds.
pub fn parse_duration(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if !text.contains(':') {
        return text
            .parse::<f64>()
            .ok()
            .filter(|s| s.is_finite() && *s > 0.0);
    }
    let mut seconds = 0.0;
    for part in text.split(':') {
        if part.is_empty() || part.len() > 2 || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        seconds = seconds * 60.0 + part.parse::<f64>().ok()?;
    }
    let parts = text.split(':').count();
    (parts <= 3 && seconds > 0.0).then_some(seconds)
}

/// Whether a line is only a duration ("3:45").
pub fn is_duration(text: &str) -> bool {
    let text = text.trim();
    text.contains(':') && parse_duration(text).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("3:45"), Some(225.0));
        assert_eq!(parse_duration("1:02:03"), Some(3723.0));
        assert_eq!(parse_duration(" 245 "), Some(245.0));
        assert_eq!(parse_duration("3:4x"), None);
        assert_eq!(parse_duration("abc"), None);
        assert_eq!(parse_duration("0:00"), None);
        assert!(is_duration("12:01"));
        assert!(!is_duration("1999"));
    }

    #[test]
    fn swapping_needs_one_artist() {
        let mut track = ImportedTrack::new("A", vec!["B".into()]);
        assert_eq!(track.swapped(), None);
        track.swappable = true;
        let swapped = track.swapped().unwrap();
        assert_eq!(
            (swapped.title.as_str(), swapped.artists[0].as_str()),
            ("B", "A")
        );
        assert_eq!(track.display(), "B — A");
    }
}
