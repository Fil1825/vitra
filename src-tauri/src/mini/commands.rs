use tauri::{AppHandle, State};

use super::MiniPlayer;
use crate::error::AppError;

/// Opens the mini player (async: creating a window from a synchronous command deadlocks on
/// Windows).
#[tauri::command]
pub async fn mini_show(mini: State<'_, MiniPlayer>) -> Result<(), AppError> {
    mini.show().await
}

#[tauri::command]
pub async fn mini_hide(mini: State<'_, MiniPlayer>) -> Result<(), AppError> {
    mini.hide()
}

/// ⌥⌘M / Ctrl+Alt+M in either window.
#[tauri::command]
pub async fn mini_toggle(mini: State<'_, MiniPlayer>) -> Result<(), AppError> {
    mini.toggle().await
}

/// "Expand" in the mini player: back to the main window.
#[tauri::command]
pub async fn mini_expand(mini: State<'_, MiniPlayer>) -> Result<(), AppError> {
    mini.switch_to_main()
}

/// Quits from the mini player, which has no menu bar or taskbar entry of its own.
#[tauri::command]
pub fn app_quit(app: AppHandle) {
    app.exit(0);
}
