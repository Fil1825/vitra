//! My Wave's commands.

use mood::Mode;
use tauri::State;

use super::{AnalysisStatus, StartOutcome, WaveService};
use crate::error::AppError;

/// How far the background analysis has come and what it is doing.
#[tauri::command]
pub fn wave_analysis(wave: State<'_, WaveService>) -> AnalysisStatus {
    wave.analysis()
}

/// Starts My Wave in this mood (replacing the queue).
#[tauri::command]
pub async fn wave_start(
    wave: State<'_, WaveService>,
    mode: Mode,
) -> Result<StartOutcome, AppError> {
    wave.begin(mode).await
}

/// "Don't like" this track: the wave leaves it out from now on.
#[tauri::command]
pub async fn wave_dislike(wave: State<'_, WaveService>, track_id: i64) -> Result<(), AppError> {
    wave.dislike(track_id).await
}

/// The wave stops picking; its queue stays.
#[tauri::command]
pub fn wave_stop(wave: State<'_, WaveService>) -> Result<(), AppError> {
    wave.end()
}
