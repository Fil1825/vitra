//! Tauri commands for playback. Each returns quickly; the resulting state change arrives as a
//! `player://state` event.

use std::path::PathBuf;

use tauri::State;

use super::{Player, PlayerState};
use crate::error::AppError;

#[tauri::command]
pub fn player_state(player: State<'_, Player>) -> PlayerState {
    player.state()
}

#[tauri::command]
pub async fn player_open(player: State<'_, Player>, path: PathBuf) -> Result<(), AppError> {
    player.open(path).await
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
