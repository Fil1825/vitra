//! My Wave's commands.

use tauri::State;

use super::{AnalysisStatus, WaveService};

/// How far the background analysis has come and what it is doing.
#[tauri::command]
pub fn wave_analysis(wave: State<'_, WaveService>) -> AnalysisStatus {
    wave.analysis()
}
