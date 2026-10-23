//! Rows returned to the UI. Field names serialize as camelCase for the frontend.

use media::TrackMetadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub added_at: i64,
    pub track_count: i64,
}

/// One of a track's artists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtistRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    /// The artists' names, joined for display ("A, B").
    pub artist: Option<String>,
    /// The main (first) artist.
    pub artist_id: Option<i64>,
    /// Every artist in credit order, the main one first.
    pub artists: Vec<ArtistRef>,
    pub album: Option<String>,
    pub album_id: Option<i64>,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub year: Option<i64>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub duration: Option<f64>,
    pub format: Option<String>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub bitrate: Option<i64>,
    pub favorite: bool,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
    pub added_at: i64,
    /// Cover thumbnail id: the track's own (a link's thumbnail), else its album's.
    pub cover: Option<String>,
    /// The library folder the file is in; `None` for a single file added on its own (opened
    /// or dropped) or a link, which only leave the library when the user removes them.
    pub folder_id: Option<i64>,
    /// The web page the track comes from. While it streams, `path` is this URL; once
    /// downloaded, `path` is the file.
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<i64>,
    pub year: Option<i64>,
    /// The cover the user chose, else the automatic one.
    pub cover: Option<String>,
    /// `cover` is one the user chose.
    pub custom_cover: bool,
    pub track_count: i64,
    /// Tracks that stream from links (and can be downloaded).
    pub link_count: i64,
    pub duration: f64,
    pub added_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: i64,
    pub name: String,
    pub album_count: i64,
    pub track_count: i64,
    /// The picture the user chose, else a cover to illustrate the artist (from one of their
    /// albums).
    pub cover: Option<String>,
    /// `cover` is one the user chose.
    pub custom_cover: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
    pub duration: f64,
    pub created_at: i64,
    pub updated_at: i64,
    /// The web playlist it was imported from.
    pub source_url: Option<String>,
    /// The cover the user chose, else the first one among its tracks.
    pub cover: Option<String>,
    /// `cover` is one the user chose.
    pub custom_cover: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub track: Track,
    pub played_at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
}

/// A file as found by the scanner, ready to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct ScannedTrack {
    pub path: String,
    pub size: u64,
    /// Unix seconds.
    pub mtime: i64,
    pub metadata: TrackMetadata,
}

/// A track that plays from a link, as yt-dlp described it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoteTrack {
    pub url: String,
    pub title: String,
    /// The credit as one text; split like a file's (see [`crate::ArtistSplitting`]) unless
    /// `artists` lists them.
    pub artist: Option<String>,
    /// The artists as the site lists them, when it does.
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track_number: Option<u32>,
    pub year: Option<u16>,
    pub duration: Option<f64>,
    /// Described from the track's own page, not from a playlist listing: its values replace
    /// what the library has. Listings only fill in tracks the library doesn't know yet.
    pub complete: bool,
}

/// Which tracks to list.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackFilter {
    All,
    Album(i64),
    /// The artist's own tracks: those they lead or whose album is theirs.
    Artist(i64),
    /// Tracks the artist only features on.
    ArtistAppearances(i64),
    Favorites,
    /// Newest first.
    RecentlyAdded {
        limit: i64,
    },
    Playlist(i64),
    /// In the given order; unknown ids are skipped.
    Ids(Vec<i64>),
}
