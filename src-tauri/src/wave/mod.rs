//! My Wave (stage 10): the library measured in the background (`analysis`) and the endless
//! queue that picks from those measurements by mood (`mood::Wave`).

mod analysis;
mod commands;
mod process;

use tauri::AppHandle;

pub use analysis::{AnalysisStatus, Analyzer};
pub use commands::*;

#[derive(Clone)]
pub struct WaveService {
    analyzer: Analyzer,
}

impl WaveService {
    pub fn new(app: AppHandle) -> Self {
        Self {
            analyzer: Analyzer::new(app),
        }
    }

    pub fn start(&self) {
        self.analyzer.start();
    }

    /// Quitting: no FFmpeg is left behind.
    pub fn stop(&self) {
        self.analyzer.stop();
    }

    /// Something the analysis waits on may have changed (its settings, power saving).
    pub fn settings_changed(&self) {
        self.analyzer.wake();
    }

    pub fn analysis(&self) -> AnalysisStatus {
        self.analyzer.status()
    }
}
