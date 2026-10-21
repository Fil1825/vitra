//! Tauri commands for the library, playlists and the tag editor.

use std::path::PathBuf;

use library::{Album, AlbumSort, Artist, Folder, HistoryEntry, Playlist, SearchResults, Track};
use media::{CoverChange, TagValues};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{Change, LibraryService, ScanStatus};
use crate::error::AppError;
use crate::player::Player;

/// Which tracks a view lists.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrackQuery {
    All,
    Album { id: i64 },
    Artist { id: i64 },
    Favorites,
    Recent { limit: i64 },
    Playlist { id: i64 },
    Search { query: String },
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

#[tauri::command]
pub async fn library_album(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Option<Album>, AppError> {
    library.run(move |db| db.album(id)).await
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
    library: State<'_, LibraryService>,
    track_id: i64,
    favorite: bool,
) -> Result<(), AppError> {
    library
        .run(move |db| db.set_favorite(track_id, favorite))
        .await?;
    library.changed(Change::Tracks);
    Ok(())
}

#[tauri::command]
pub async fn library_track(
    library: State<'_, LibraryService>,
    id: i64,
) -> Result<Option<Track>, AppError> {
    library.run(move |db| db.track(id)).await
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

/// The tag editor's form, read from the file itself.
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
}

#[tauri::command]
pub async fn tags_load(player: State<'_, Player>, path: PathBuf) -> Result<TagForm, AppError> {
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
    path: PathBuf,
    values: TagValues,
    cover: CoverEdit,
) -> Result<(), AppError> {
    let library = library.inner().clone();
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
