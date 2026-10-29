//! Tauri commands for the library, playlists and the tag editor.

use std::path::PathBuf;

use library::{
    Album, AlbumSort, Artist, CoverTarget, Folder, HistoryEntry, Playlist, SearchResults, Track,
};
use media::{CoverChange, TagValues};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::{Change, LibraryService, ScanStatus};
use crate::error::AppError;
use crate::links::LinkService;
use crate::player::Player;
use crate::wave::WaveService;

/// Which tracks a view lists.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrackQuery {
    All,
    Album {
        id: i64,
    },
    Artist {
        id: i64,
    },
    /// Tracks the artist only features on.
    ArtistAppearances {
        id: i64,
    },
    Favorites,
    Recent {
        limit: i64,
    },
    Playlist {
        id: i64,
    },
    Search {
        query: String,
    },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AlbumOrder {
    Artist,
    Recent,
}

#[tauri::command]
pub async fn library_folders(library: State<'_, LibraryService>) -> Result<Vec<Folder>, AppError> {
    library.run(|db| db.folders()).await
}

#[tauri::command]
pub async fn library_add_folder(
    library: State<'_, LibraryService>,
    path: String,
) -> Result<Folder, AppError> {
    let library = library.inner().clone();
    tauri::async_runtime::spawn_blocking(move || library.add_folder(&path)).await?
}

#[tauri::command]
pub async fn library_remove_folder(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<(), AppError> {
    let library = library.inner().clone();
    tauri::async_runtime::spawn_blocking(move || library.remove_folder(id)).await?
}

/// Rescans one folder, or all of them.
#[tauri::command]
pub async fn library_rescan(
    library: State<'_, LibraryService>,
    id: Option<i64>,
) -> Result<(), AppError> {
    let ids = match id {
        Some(id) => vec![id],
        None => library
            .run(|db| db.folders())
            .await?
            .into_iter()
            .map(|f| f.id)
            .collect(),
    };
    for id in ids {
        library.request_scan(id);
    }
    Ok(())
}

#[tauri::command]
pub fn library_scan_status(library: State<'_, LibraryService>) -> ScanStatus {
    library.scan_status()
}

#[tauri::command]
pub async fn library_tracks(
    library: State<'_, LibraryService>,
    query: TrackQuery,
) -> Result<Vec<Track>, AppError> {
    library
        .run(move |db| {
            use library::TrackFilter as F;
            match query {
                TrackQuery::All => db.tracks(&F::All),
                TrackQuery::Album { id } => db.tracks(&F::Album(id)),
                TrackQuery::Artist { id } => db.tracks(&F::Artist(id)),
                TrackQuery::ArtistAppearances { id } => db.tracks(&F::ArtistAppearances(id)),
                TrackQuery::Favorites => db.tracks(&F::Favorites),
                TrackQuery::Recent { limit } => db.tracks(&F::RecentlyAdded { limit }),
                TrackQuery::Playlist { id } => db.tracks(&F::Playlist(id)),
                TrackQuery::Search { query } => db.search_tracks(&query),
            }
        })
        .await
}

#[tauri::command]
pub async fn library_albums(
    library: State<'_, LibraryService>,
    order: AlbumOrder,
) -> Result<Vec<Album>, AppError> {
    let sort = match order {
        AlbumOrder::Artist => AlbumSort::Artist,
        AlbumOrder::Recent => AlbumSort::RecentlyAdded,
    };
    library.run(move |db| db.albums(sort)).await
}

/// An album; one of links gets a cover looked for if it has none, and its tracks numbered
/// from its listing if some lack numbers, in the background.
#[tauri::command]
pub async fn library_album(
    library: State<'_, LibraryService>,
    links: State<'_, crate::links::LinkService>,
    id: i64,
) -> Result<Option<Album>, AppError> {
    let album = library.run(move |db| db.album(id)).await?;
    if album.as_ref().is_some_and(|album| album.cover.is_none()) {
        links.cover_album_later(id);
    }
    // Downloaded copies may need numbers too, so not only albums with streams.
    if album.is_some() {
        links.renumber_album_later(id);
    }
    Ok(album)
}

/// Image files bigger than this aren't covers.
const MAX_COVER_FILE: u64 = 50 * 1024 * 1024;

/// Sets the cover the user chose for an album, artist or playlist from an image file, or
/// with no file goes back to the automatic one. Only the library keeps it: files' tags are
/// the tag editor's business.
#[tauri::command]
pub async fn library_set_cover(
    library: State<'_, LibraryService>,
    player: State<'_, Player>,
    target: CoverTarget,
    path: Option<PathBuf>,
) -> Result<Option<String>, AppError> {
    let service = library.inner().clone();
    let covers = service.covers_dir().to_path_buf();
    let (cover, album_tracks) = service
        .run(move |db| {
            let image = match path {
                Some(path) => {
                    if std::fs::metadata(&path)?.len() > MAX_COVER_FILE {
                        return Err(library::Error::Invalid("the image is too big".into()));
                    }
                    Some(std::fs::read(path)?)
                }
                None => None,
            };
            let cover = db.set_custom_cover(target, image.as_deref(), &covers)?;
            let album_tracks = match target {
                CoverTarget::Album(id) => db.tracks(&library::TrackFilter::Album(id))?,
                CoverTarget::Artist(_) | CoverTarget::Playlist(_) => Vec::new(),
            };
            Ok((cover, album_tracks))
        })
        .await?;
    service.changed(Change::Tracks);
    service.changed(Change::Playlists);
    // Queue entries and the playing track show an album's cover.
    player.library_tracks_updated(&album_tracks);
    Ok(cover)
}

#[tauri::command]
pub async fn library_artists(library: State<'_, LibraryService>) -> Result<Vec<Artist>, AppError> {
    library.run(|db| db.artists()).await
}

#[tauri::command]
pub async fn library_artist(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Option<Artist>, AppError> {
    library.run(move |db| db.artist(id)).await
}

#[tauri::command]
pub async fn library_artist_albums(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Vec<Album>, AppError> {
    library.run(move |db| db.artist_albums(id)).await
}

#[tauri::command]
pub async fn library_search(
    library: State<'_, LibraryService>,
    query: String,
    limit: usize,
) -> Result<SearchResults, AppError> {
    library.run(move |db| db.search(&query, limit)).await
}

#[tauri::command]
pub async fn library_set_favorite(
    app: AppHandle,
    library: State<'_, LibraryService>,
    track_id: i64,
    favorite: bool,
) -> Result<(), AppError> {
    library
        .run(move |db| db.set_favorite(track_id, favorite))
        .await?;
    library.changed(Change::Tracks);
    // The heart is My Wave's "like".
    if favorite && let Some(wave) = app.try_state::<WaveService>() {
        wave.liked(track_id);
    }
    Ok(())
}

#[tauri::command]
pub async fn library_track(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Option<Track>, AppError> {
    library.run(move |db| db.track(id)).await
}

/// Removes single tracks (added on their own) from the library; their files stay on disk.
#[tauri::command]
pub async fn library_remove_tracks(
    library: State<'_, LibraryService>,
    player: State<'_, Player>,
    track_ids: Vec<i64>,
) -> Result<usize, AppError> {
    let service = library.inner().clone();
    let ids = track_ids.clone();
    let removed =
        tauri::async_runtime::spawn_blocking(move || service.remove_single_tracks(&ids)).await??;
    player.tracks_removed(&track_ids);
    Ok(removed)
}

#[tauri::command]
pub async fn library_history(
    library: State<'_, LibraryService>,
    limit: i64,
) -> Result<Vec<HistoryEntry>, AppError> {
    library.run(move |db| db.history(limit)).await
}

#[tauri::command]
pub async fn playlists_list(library: State<'_, LibraryService>) -> Result<Vec<Playlist>, AppError> {
    library.run(|db| db.playlists()).await
}

#[tauri::command]
pub async fn playlist_get(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Option<Playlist>, AppError> {
    library.run(move |db| db.playlist(id)).await
}

/// Runs a playlist edit and tells the windows.
async fn edit_playlists<T: Send + 'static>(
    library: &LibraryService,
    f: impl FnOnce(&mut library::Library) -> library::Result<T> + Send + 'static,
) -> Result<T, AppError> {
    let result = library.run(f).await?;
    library.changed(Change::Playlists);
    Ok(result)
}

#[tauri::command]
pub async fn playlist_create(
    library: State<'_, LibraryService>,
    name: String,
    track_ids: Option<Vec<i64>>,
) -> Result<Playlist, AppError> {
    edit_playlists(&library, move |db| {
        let playlist = db.create_playlist(&name)?;
        if let Some(ids) = track_ids.filter(|ids| !ids.is_empty()) {
            db.add_to_playlist(playlist.id, &ids, None)?;
        }
        Ok(db.playlist(playlist.id)?.unwrap_or(playlist))
    })
    .await
}

#[tauri::command]
pub async fn playlist_rename(
    library: State<'_, LibraryService>,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    edit_playlists(&library, move |db| db.rename_playlist(id, &name)).await
}

#[tauri::command]
pub async fn playlist_delete(library: State<'_, LibraryService>, id: i64) -> Result<(), AppError> {
    edit_playlists(&library, move |db| db.delete_playlist(id)).await
}

#[tauri::command]
pub async fn playlist_add(
    library: State<'_, LibraryService>,
    id: i64,
    track_ids: Vec<i64>,
    index: Option<usize>,
) -> Result<(), AppError> {
    edit_playlists(&library, move |db| {
        db.add_to_playlist(id, &track_ids, index)
    })
    .await
}

#[tauri::command]
pub async fn playlist_remove(
    library: State<'_, LibraryService>,
    id: i64,
    indices: Vec<usize>,
) -> Result<(), AppError> {
    edit_playlists(&library, move |db| db.remove_from_playlist(id, &indices)).await
}

#[tauri::command]
pub async fn playlist_move(
    library: State<'_, LibraryService>,
    id: i64,
    from: usize,
    to: usize,
) -> Result<(), AppError> {
    edit_playlists(&library, move |db| db.move_in_playlist(id, from, to)).await
}

/// The tag editor's form, read from the file itself (a link's from the library).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagForm {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub year: Option<u16>,
    /// Artwork id of the current cover (served by `artwork://`).
    pub cover: Option<String>,
    /// Whether the file's format can be tagged (lofty can read it).
    pub writable: bool,
    /// A streamed link: no file, so the library keeps the corrections (no cover here).
    pub link: bool,
    /// A link the user corrected: its page's details can be restored.
    pub edited: bool,
}

/// The path of a streamed link, if `path` is one.
fn link_url(path: &std::path::Path) -> Option<String> {
    path.to_str()
        .filter(|path| links::is_remote(path))
        .map(str::to_owned)
}

#[tauri::command]
pub async fn tags_load(
    library: State<'_, LibraryService>,
    player: State<'_, Player>,
    path: PathBuf,
) -> Result<TagForm, AppError> {
    if let Some(url) = link_url(&path) {
        let key = url.clone();
        let (track, edited) = library
            .run(move |db| Ok((db.track_by_path(&key)?, db.link_edited(&key)?)))
            .await?;
        let track =
            track.ok_or_else(|| AppError::NotFound(format!("{url} is not in the library")))?;
        // Several artists are edited as "A; B", like a file's.
        let artist = if track.artists.len() > 1 {
            track
                .artists
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(&format!("{} ", media::ARTIST_SEPARATOR))
        } else {
            track.artist.unwrap_or_default()
        };
        return Ok(TagForm {
            title: track.title,
            artist,
            album: track.album.unwrap_or_default(),
            album_artist: track.album_artist.unwrap_or_default(),
            year: track.year.and_then(|year| u16::try_from(year).ok()),
            cover: None,
            writable: true,
            link: true,
            edited,
        });
    }
    let read = tauri::async_runtime::spawn_blocking(move || {
        media::read_track(&path).map(|info| {
            let cover = info
                .embedded_artwork
                .and_then(|art| media::prepare_artwork(art).ok());
            (info.metadata, cover)
        })
    })
    .await?;
    let (metadata, cover) = match read {
        Ok(read) => read,
        Err(err) => {
            log::info!("tag editor: {err}");
            return Ok(TagForm {
                title: String::new(),
                artist: String::new(),
                album: String::new(),
                album_artist: String::new(),
                year: None,
                cover: None,
                writable: false,
                link: false,
                edited: false,
            });
        }
    };
    Ok(TagForm {
        title: metadata.title.unwrap_or_default(),
        artist: metadata.artist.unwrap_or_default(),
        album: metadata.album.unwrap_or_default(),
        album_artist: metadata.album_artist.unwrap_or_default(),
        year: metadata.year,
        cover: cover.map(|art| player.artwork().insert(art)),
        writable: true,
        link: false,
        edited: false,
    })
}

/// A cover image picked in the tag editor, prepared for preview; returns its artwork id.
#[tauri::command]
pub async fn artwork_preview(player: State<'_, Player>, path: PathBuf) -> Result<String, AppError> {
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(&path)?;
        media::prepare_artwork(media::Artwork { bytes, mime: None }).map_err(AppError::from)
    })
    .await??;
    Ok(player.artwork().insert(prepared))
}

/// Whose artwork to show full size.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArtworkSource {
    /// A file or link: its embedded cover, the cover next to it, or a link's thumbnail.
    Track {
        path: String,
    },
    Album {
        id: i64,
    },
}

/// The largest image of a file: embedded, else a cover file next to it.
fn file_artwork(path: &std::path::Path) -> Option<media::Artwork> {
    media::read_track(path)
        .ok()
        .and_then(|info| info.embedded_artwork)
        .or_else(|| media::folder_artwork(path))
}

/// Artwork in its original size (the player's copies are downscaled), for the full-screen
/// viewer; returns its artwork id, or `None` when there is none.
#[tauri::command]
pub async fn artwork_original(
    library: State<'_, LibraryService>,
    links: State<'_, crate::links::LinkService>,
    player: State<'_, Player>,
    source: ArtworkSource,
) -> Result<Option<String>, AppError> {
    let library = library.inner().clone();
    let covers = library.covers_dir().to_path_buf();
    // A link's thumbnail, if one was seen recently.
    let thumbnail = match &source {
        ArtworkSource::Track { path } if links::is_remote(path) => links.thumbnail(path),
        _ => None,
    };
    let artwork = tauri::async_runtime::spawn_blocking(move || -> Result<_, AppError> {
        let cached = |id: String| {
            let bytes = std::fs::read(covers.join(format!("{id}.jpg"))).ok()?;
            Some(media::Artwork {
                bytes,
                mime: Some("image/jpeg".into()),
            })
        };
        // The cover the user chose for the album comes first.
        let custom = match &source {
            ArtworkSource::Track { path } => {
                let key = path.clone();
                library.with(|db| db.custom_album_cover(&key))?
            }
            ArtworkSource::Album { id } => {
                let id = *id;
                library.with(|db| db.custom_cover_of_album(id))?
            }
        };
        if let Some(artwork) = custom.and_then(cached) {
            return Ok(Some(artwork));
        }
        if let Some(bytes) = thumbnail {
            return Ok(Some(media::Artwork {
                bytes: bytes.as_ref().clone(),
                mime: None,
            }));
        }
        let (paths, cover) = match source {
            ArtworkSource::Track { path } => {
                let key = path.clone();
                let track = library.with(|db| db.track_by_path(&key))?;
                let paths = if links::is_remote(&path) {
                    Vec::new()
                } else {
                    vec![path]
                };
                (paths, track.and_then(|t| t.cover))
            }
            ArtworkSource::Album { id } => {
                let paths = library.with(|db| db.album_paths(id))?;
                let cover = library.with(|db| db.album(id))?.and_then(|a| a.cover);
                (paths, cover)
            }
        };
        if let Some(artwork) = paths
            .iter()
            .take(3)
            .find_map(|path| file_artwork(std::path::Path::new(path)))
        {
            return Ok(Some(artwork));
        }
        // Links and albums of links only have the library's cached copy.
        Ok(cover.and_then(cached))
    })
    .await??;
    Ok(artwork.map(|artwork| player.artwork().insert_original(artwork)))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CoverEdit {
    Keep,
    Remove,
    Replace { path: PathBuf },
}

#[tauri::command]
pub async fn tags_save(
    library: State<'_, LibraryService>,
    player: State<'_, Player>,
    links: State<'_, LinkService>,
    path: PathBuf,
    values: TagValues,
    cover: CoverEdit,
) -> Result<(), AppError> {
    let library = library.inner().clone();
    if let Some(url) = link_url(&path) {
        // A link has no file: the library keeps the corrections, and a download writes them.
        let non_empty = |value: &str| Some(value.trim().to_owned()).filter(|v| !v.is_empty());
        let edit = library::LinkEdit {
            title: values.title.clone(),
            artists: media::artist_list(&values.artist)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            album: non_empty(&values.album),
            album_artist: non_empty(&values.album_artist),
            year: values.year,
        };
        let covers = library.covers_dir().to_path_buf();
        // The full thumbnail, if the link was seen lately, makes a sharper album cover.
        let thumbnail = links.thumbnail(&url);
        let track = library
            .run(move |db| {
                db.edit_link(
                    &url,
                    &edit,
                    thumbnail.as_deref().map(Vec::as_slice),
                    &covers,
                )
            })
            .await?;
        library.changed(Change::Tracks);
        library.changed(Change::Playlists);
        player.library_tracks_updated(&[track]);
        return Ok(());
    }
    let target = path.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<(), AppError> {
        let cover = match cover {
            CoverEdit::Keep => CoverChange::Keep,
            CoverEdit::Remove => CoverChange::Remove,
            CoverEdit::Replace { path } => CoverChange::Replace(std::fs::read(path)?),
        };
        media::write_tags(&target, &values, &cover)?;
        library.refresh_files(&[&target])
    })
    .await??;
    player.file_retagged(path).await;
    Ok(())
}
