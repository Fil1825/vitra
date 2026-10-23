//! Tauri commands for playback and the queue. Each returns quickly; the resulting changes
//! arrive as `player://state` and `queue://changed` events.

use std::collections::HashSet;
use std::path::PathBuf;

use tauri::State;

use super::{Player, PlayerState, QueueView, Repeat};
use crate::error::AppError;
use crate::library::LibraryService;

#[tauri::command]
pub fn player_state(player: State<'_, Player>) -> PlayerState {
    player.state()
}

/// Plays files and folders, replacing the queue.
#[tauri::command]
pub async fn player_open(player: State<'_, Player>, paths: Vec<PathBuf>) -> Result<(), AppError> {
    player.open(paths).await
}

/// A window starts or stops showing the backdrop's pulse (see [`Player::set_level_meter`]).
#[tauri::command]
pub fn player_level_meter(player: State<'_, Player>, enabled: bool) {
    player.set_level_meter(enabled);
}

#[tauri::command]
pub async fn player_play(player: State<'_, Player>) -> Result<(), AppError> {
    player.play().await
}

#[tauri::command]
pub async fn player_pause(player: State<'_, Player>) -> Result<(), AppError> {
    player.pause().await
}

#[tauri::command]
pub async fn player_toggle(player: State<'_, Player>) -> Result<(), AppError> {
    player.toggle().await
}

#[tauri::command]
pub async fn player_next(player: State<'_, Player>) -> Result<(), AppError> {
    player.next().await
}

#[tauri::command]
pub async fn player_previous(player: State<'_, Player>) -> Result<(), AppError> {
    player.previous().await
}

#[tauri::command]
pub async fn player_seek(player: State<'_, Player>, position: f64) -> Result<(), AppError> {
    player.seek(position).await
}

#[tauri::command]
pub async fn player_set_volume(player: State<'_, Player>, volume: f64) -> Result<(), AppError> {
    player.set_volume(volume).await
}

#[tauri::command]
pub async fn player_set_muted(player: State<'_, Player>, muted: bool) -> Result<(), AppError> {
    player.set_muted(muted).await
}

#[tauri::command]
pub async fn player_set_speed(player: State<'_, Player>, speed: f64) -> Result<(), AppError> {
    player.set_speed(speed).await
}

#[tauri::command]
pub fn queue_get(player: State<'_, Player>) -> QueueView {
    player.queue()
}

/// Replaces the queue with library tracks and plays the one at `start`.
#[tauri::command]
pub async fn queue_play(
    player: State<'_, Player>,
    library: State<'_, LibraryService>,
    track_ids: Vec<i64>,
    start: usize,
) -> Result<(), AppError> {
    let entries = library.entries(track_ids.clone()).await?;
    // Tracks deleted meanwhile are skipped; count only the ones before `start` that remain.
    let found: HashSet<i64> = entries.iter().filter_map(|e| e.track_id).collect();
    let start = track_ids
        .iter()
        .take(start)
        .filter(|id| found.contains(id))
        .count();
    player.play_entries(entries, start).await
}

/// Adds library tracks after the current item (`next`) or at the end.
#[tauri::command]
pub async fn queue_add(
    player: State<'_, Player>,
    library: State<'_, LibraryService>,
    track_ids: Vec<i64>,
    next: bool,
) -> Result<(), AppError> {
    let entries = library.entries(track_ids).await?;
    player.enqueue(entries, next).await
}

#[tauri::command]
pub async fn queue_jump(player: State<'_, Player>, uid: u64) -> Result<(), AppError> {
    player.jump(uid).await
}

#[tauri::command]
pub async fn queue_remove(player: State<'_, Player>, uids: Vec<u64>) -> Result<(), AppError> {
    player.remove(uids).await
}

#[tauri::command]
pub async fn queue_move(player: State<'_, Player>, from: usize, to: usize) -> Result<(), AppError> {
    player.move_item(from, to).await
}

/// Removes everything after the current item.
#[tauri::command]
pub async fn queue_clear(player: State<'_, Player>) -> Result<(), AppError> {
    player.clear_upcoming().await
}

#[tauri::command]
pub async fn queue_set_shuffle(player: State<'_, Player>, shuffle: bool) -> Result<(), AppError> {
    player.set_shuffle(shuffle).await
}

#[tauri::command]
pub async fn queue_set_repeat(player: State<'_, Player>, repeat: Repeat) -> Result<(), AppError> {
    player.set_repeat(repeat).await
}
