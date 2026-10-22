//! Tauri commands for links, downloads and yt-dlp.

use library::{Playlist, TrackFilter};
use tauri::{AppHandle, State};

use super::downloads::{self, Job};
use super::{Downloads, LinkPreview, LinkService, PlayMode, YtDlpInfo};
use crate::error::AppError;
use crate::library::LibraryService;
use crate::settings::SettingsStore;

/// What a pasted link is (a track or a playlist), for the link dialog.
#[tauri::command]
pub async fn link_resolve(
    links: State<'_, LinkService>,
    input: String,
) -> Result<LinkPreview, AppError> {
    links.resolve(&input).await
}

#[tauri::command]
pub async fn link_play(
    links: State<'_, LinkService>,
    url: String,
    mode: PlayMode,
) -> Result<(), AppError> {
    links.play(&url, mode).await
}

#[tauri::command]
pub async fn link_import(links: State<'_, LinkService>, url: String) -> Result<Playlist, AppError> {
    links.import(&url).await
}

/// Downloads a link (every entry, for a playlist); returns how many downloads were queued.
#[tauri::command]
pub async fn link_download(
    links: State<'_, LinkService>,
    downloads: State<'_, Downloads>,
    url: String,
) -> Result<usize, AppError> {
    let tracks = links.tracks_for_download(&url).await?;
    Ok(downloads.add(tracks))
}

#[tauri::command]
pub fn downloads_list(downloads: State<'_, Downloads>) -> Vec<Job> {
    downloads.jobs()
}

/// Downloads library tracks that are links; returns how many were queued.
#[tauri::command]
pub async fn downloads_add(
    library: State<'_, LibraryService>,
    downloads: State<'_, Downloads>,
    track_ids: Vec<i64>,
) -> Result<usize, AppError> {
    let tracks = library
        .run(move |db| db.tracks(&TrackFilter::Ids(track_ids)))
        .await?;
    Ok(downloads.add(tracks))
}

#[tauri::command]
pub fn downloads_cancel(downloads: State<'_, Downloads>, id: u64) {
    downloads.cancel(id);
}

#[tauri::command]
pub fn downloads_retry(downloads: State<'_, Downloads>, id: u64) {
    downloads.retry(id);
}

#[tauri::command]
pub fn downloads_clear(downloads: State<'_, Downloads>) {
    downloads.clear();
}

/// Where downloads go with the current settings.
#[tauri::command]
pub fn downloads_folder(
    app: AppHandle,
    settings: State<'_, SettingsStore>,
) -> Result<String, AppError> {
    let folder = downloads::download_folder(&app, &settings.get().downloads)?;
    Ok(folder.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn ytdlp_info(links: State<'_, LinkService>) -> Result<YtDlpInfo, AppError> {
    Ok(links.ytdlp_info().await)
}

/// Updates yt-dlp; returns its new version.
#[tauri::command]
pub async fn ytdlp_update(links: State<'_, LinkService>) -> Result<String, AppError> {
    links.update_ytdlp().await
}
