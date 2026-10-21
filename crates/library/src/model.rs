//! Rows returned to the UI. Field names serialize as camelCase for the frontend.

use media::TrackMetadata;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub path: String,
    pub added_at: i64,
    pub track_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<i64>,
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
    /// Cover thumbnail id of the track's album.
    pub cover: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub title: String,
    pub artist: Option<String>,
    pub artist_id: Option<i64>,
    pub year: Option<i64>,
    pub cover: Option<String>,
    pub track_count: i64,
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
    /// A cover to illustrate the artist (from one of their albums).
    pub cover: Option<String>,
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

/// Which tracks to list.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackFilter {
    All,
    Album(i64),
    Artist(i64),
    Favorites,
    /// Newest first.
    RecentlyAdded {
        limit: i64,
    },
    Playlist(i64),
    /// In the given order; unknown ids are skipped.
    Ids(Vec<i64>),
}
