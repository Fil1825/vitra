//! Playlist import (stage 9): reading a playlist from whatever the user has (a file, pasted
//! text, a service's page) and matching its tracks against the library and YouTube Music.
//! Pure: fetching pages and running searches are the app's.

pub mod files;
pub mod links;
pub mod matching;
pub mod model;
pub mod normalize;
pub mod pages;
mod table;
pub mod text;

pub use model::{ImportedPlaylist, ImportedTrack};
