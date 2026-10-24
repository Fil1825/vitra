//! The Music app's playlists (macOS), read through Apple's iTunesLibrary framework: the app
//! no longer writes the XML file iTunes kept next to the library. The first read asks the
//! user for access to the media library (`NSAppleMusicUsageDescription` in `Info.plist`).

use serde::Serialize;

use crate::error::AppError;
use import::ImportedPlaylist;

/// A playlist the import dialog offers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicPlaylist {
    /// The playlist's persistent id, as 16 hex digits.
    pub id: String,
    pub name: String,
    /// The folder it sits in, if any.
    pub folder: Option<String>,
    pub tracks: usize,
    pub smart: bool,
}

/// What an imported playlist keeps as its source (before the id), so importing it again,
/// or "Refresh" on it, updates the same playlist.
pub const SOURCE_PREFIX: &str = "music-app:playlist/";

pub fn source_url(id: &str) -> String {
    format!("{SOURCE_PREFIX}{id}")
}

/// The user's playlists in the Music app.
pub async fn playlists() -> Result<Vec<MusicPlaylist>, AppError> {
    run(platform::playlists).await
}

/// The playlists with these ids, with their tracks.
pub async fn read(ids: Vec<String>) -> Result<Vec<(String, ImportedPlaylist)>, AppError> {
    run(move || platform::read(&ids)).await
}

/// Opening the library parses all of it (seconds for a big one), so it runs off the async
/// runtime.
async fn run<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| AppError::Task(err.to_string()))?
}

#[cfg(target_os = "macos")]
mod platform {
    use std::collections::HashMap;

    use import::{ImportedPlaylist, ImportedTrack};
    use objc2::rc::{Retained, autoreleasepool};
    use objc2_foundation::NSString;
    use objc2_itunes_library::{
        ITLibDistinguishedPlaylistKind as Kind, ITLibMediaItem, ITLibPlaylist, ITLibPlaylistKind,
        ITLibrary,
    };

    use super::MusicPlaylist;
    use crate::error::AppError;

    // SAFETY (all `unsafe` blocks below): iTunesLibrary's own API on objects it returned,
    // used on one thread and dropped inside the autorelease pool.

    fn library() -> Result<Retained<ITLibrary>, AppError> {
        #[allow(unsafe_code)]
        unsafe { ITLibrary::libraryWithAPIVersion_error(&NSString::from_str("1.1")) }.map_err(
            |err| {
                AppError::Import(format!(
                    "cannot read the Music library: {}",
                    err.localizedDescription()
                ))
            },
        )
    }

    fn id(playlist: &ITLibPlaylist) -> String {
        #[allow(unsafe_code)]
        let id = unsafe { playlist.persistentID() };
        format!("{:016X}", id.unsignedLongLongValue())
    }

    /// Playlists the user made or sees under "Playlists": not the whole library, its
    /// sections (Music, Podcasts, Purchased…) or folders.
    fn listed(playlist: &ITLibPlaylist) -> bool {
        #[allow(unsafe_code)]
        let (primary, visible, kind, distinguished) = unsafe {
            (
                playlist.isPrimary(),
                playlist.isVisible(),
                playlist.kind(),
                playlist.distinguishedKind(),
            )
        };
        let shown_kind = matches!(
            distinguished,
            Kind::KindNone
                | Kind::Kind90sMusic
                | Kind::KindMyTopRated
                | Kind::KindTop25MostPlayed
                | Kind::KindRecentlyPlayed
                | Kind::KindRecentlyAdded
                | Kind::KindClassicalMusic
                | Kind::KindLovedSongs
        );
        !primary && visible && kind != ITLibPlaylistKind::Folder && shown_kind
    }

    fn track(item: &ITLibMediaItem) -> Option<ImportedTrack> {
        #[allow(unsafe_code)]
        let (title, artist, album, time, location, video) = unsafe {
            (
                item.title().to_string(),
                item.artist().and_then(|artist| artist.name()),
                item.album().title(),
                item.totalTime(),
                item.location(),
                item.isVideo(),
            )
        };
        if title.trim().is_empty() || video {
            return None;
        }
        Some(ImportedTrack {
            album: album.map(|album| album.to_string()),
            duration: (time > 0).then(|| time as f64 / 1000.0),
            path: location
                .and_then(|url| url.to_file_path())
                .map(|path| path.to_string_lossy().into_owned()),
            ..ImportedTrack::new(title, artist.map(|a| a.to_string()).into_iter().collect())
        })
    }

    pub(super) fn playlists() -> Result<Vec<MusicPlaylist>, AppError> {
        autoreleasepool(|_| {
            let library = library()?;
            #[allow(unsafe_code)]
            let all = unsafe { library.allPlaylists() };
            // Folders by their persistent id, to name the folder a playlist sits in.
            let mut folders = HashMap::new();
            for playlist in all.iter() {
                #[allow(unsafe_code)]
                let (kind, name) = unsafe { (playlist.kind(), playlist.name()) };
                if kind == ITLibPlaylistKind::Folder {
                    folders.insert(id(&playlist), name.to_string());
                }
            }
            let mut found = Vec::new();
            for playlist in all.iter().filter(|playlist| listed(playlist)) {
                #[allow(unsafe_code)]
                let (name, parent, count, kind) = unsafe {
                    (
                        playlist.name().to_string(),
                        playlist.parentID(),
                        playlist.items().count(),
                        playlist.kind(),
                    )
                };
                let folder = parent
                    .map(|parent| format!("{:016X}", parent.unsignedLongLongValue()))
                    .and_then(|parent| folders.get(&parent).cloned());
                found.push(MusicPlaylist {
                    id: id(&playlist),
                    name,
                    folder,
                    tracks: count,
                    smart: kind != ITLibPlaylistKind::Regular,
                });
            }
            Ok(found)
        })
    }

    pub(super) fn read(ids: &[String]) -> Result<Vec<(String, ImportedPlaylist)>, AppError> {
        autoreleasepool(|_| {
            let library = library()?;
            #[allow(unsafe_code)]
            let all = unsafe { library.allPlaylists() };
            let mut read = Vec::new();
            for wanted in ids {
                let Some(playlist) = all.iter().find(|playlist| id(playlist) == *wanted) else {
                    continue;
                };
                #[allow(unsafe_code)]
                let (name, items) = unsafe { (playlist.name().to_string(), playlist.items()) };
                read.push((
                    wanted.clone(),
                    ImportedPlaylist {
                        name: Some(name),
                        tracks: items.iter().filter_map(|item| track(&item)).collect(),
                        ..ImportedPlaylist::default()
                    },
                ));
            }
            if read.is_empty() && !ids.is_empty() {
                return Err(AppError::Import(
                    "the playlist is no longer in the Music app".into(),
                ));
            }
            Ok(read)
        })
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use import::ImportedPlaylist;

    use super::MusicPlaylist;
    use crate::error::AppError;

    pub(super) fn playlists() -> Result<Vec<MusicPlaylist>, AppError> {
        Err(AppError::Import("the Music app is on macOS only".into()))
    }

    pub(super) fn read(_ids: &[String]) -> Result<Vec<(String, ImportedPlaylist)>, AppError> {
        Err(AppError::Import("the Music app is on macOS only".into()))
    }
}
