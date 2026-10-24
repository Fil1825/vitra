//! Import commands for the windows.

use std::path::PathBuf;

use library::{EntryCandidate, ImportProgress, PlaylistPlace};
use tauri::State;

use super::music::{self, MusicPlaylist};
use super::{ImportResult, ImportService};
use crate::error::AppError;

/// Pasted text: a link, links, or tracks written out; `name` names a playlist that has none.
/// With `into`, the text completes that playlist.
#[tauri::command]
pub async fn import_text(
    import: State<'_, ImportService>,
    text: String,
    name: String,
    into: Option<i64>,
) -> Result<ImportResult, AppError> {
    import.import_text(&text, &name, into).await
}

/// Playlist files (each may hold several playlists).
#[tauri::command]
pub async fn import_files(
    import: State<'_, ImportService>,
    paths: Vec<PathBuf>,
) -> Result<Vec<ImportResult>, AppError> {
    let mut results = Vec::new();
    for path in paths {
        results.extend(import.import_file(&path).await?);
    }
    Ok(results)
}

/// A link to a playlist, album or track list on a service; also brings an imported playlist
/// up to date with its source.
#[tauri::command]
pub async fn import_link(
    import: State<'_, ImportService>,
    url: String,
) -> Result<ImportResult, AppError> {
    import.import_link(&url).await
}

/// Every place of a playlist, entries still being looked for included.
#[tauri::command]
pub async fn import_places(
    import: State<'_, ImportService>,
    playlist_id: i64,
) -> Result<Vec<PlaylistPlace>, AppError> {
    import.places(playlist_id).await
}

#[tauri::command]
pub async fn import_progress(
    import: State<'_, ImportService>,
    playlist_id: i64,
) -> Result<ImportProgress, AppError> {
    import.progress(playlist_id).await
}

#[tauri::command]
pub async fn import_pick(
    import: State<'_, ImportService>,
    entry_id: i64,
    candidate: EntryCandidate,
) -> Result<(), AppError> {
    import.pick(entry_id, candidate).await
}

/// "Find…": the library's and YouTube Music's tracks for an entry or a typed query.
#[tauri::command]
pub async fn import_search(
    import: State<'_, ImportService>,
    entry_id: i64,
    query: Option<String>,
) -> Result<Vec<EntryCandidate>, AppError> {
    import.search(entry_id, query).await
}

/// Looks again for the tracks of a playlist that weren't found.
#[tauri::command]
pub async fn import_retry(
    import: State<'_, ImportService>,
    playlist_id: i64,
) -> Result<(), AppError> {
    import.retry(playlist_id).await
}

/// The Music app's playlists (macOS; elsewhere an error the dialog never asks for).
#[tauri::command]
pub async fn music_playlists() -> Result<Vec<MusicPlaylist>, AppError> {
    music::playlists().await
}

#[tauri::command]
pub async fn import_music(
    import: State<'_, ImportService>,
    ids: Vec<String>,
) -> Result<Vec<ImportResult>, AppError> {
    import.import_music(ids).await
}
