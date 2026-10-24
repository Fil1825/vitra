//! Click-through: the mini player lets clicks through to the windows below, except while ⌥
//! (Alt on Windows) is held with the pointer over it. Tauri can only switch a window between
//! the two, so a light loop watches the pointer and the key while click-through is on and the
//! mini player is shown.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{Emitter, WebviewWindow};

use super::{LABEL, MiniPlayer, platform};

/// Emitted to the mini player while click-through is on: whether it takes clicks right now.
pub const INTERACTIVE_EVENT: &str = "mini://interactive";

/// How often the pointer and the key are looked at: often enough to feel immediate.
const POLL: Duration = Duration::from_millis(80);

#[derive(Default)]
pub struct Passthrough {
    running: AtomicBool,
}

impl Passthrough {
    /// Starts watching unless a loop is already running.
    pub fn start(&self, mini: &MiniPlayer) {
        if !self.running.swap(true, Ordering::Relaxed) {
            tauri::async_runtime::spawn(watch(mini.clone()));
        }
    }
}

/// Click-through was switched off: the window takes clicks again.
pub fn stop(window: &WebviewWindow) {
    if let Err(err) = window.set_ignore_cursor_events(false) {
        log::warn!("cannot make the mini player clickable: {err}");
    }
    let _ = window.emit_to(LABEL, INTERACTIVE_EVENT, true);
}

async fn watch(mini: MiniPlayer) {
    let mut usable = None;
    loop {
        let Some(window) = mini.window() else {
            break;
        };
        // The settings change (and hiding) end the loop; showing it again starts another.
        if !mini.settings().click_through || !window.is_visible().unwrap_or(false) {
            break;
        }
        let now = platform::option_held() && pointer_over(&window);
        if usable != Some(now) {
            if let Err(err) = window.set_ignore_cursor_events(!now) {
                log::warn!("cannot switch click-through: {err}");
            }
            let _ = window.emit_to(LABEL, INTERACTIVE_EVENT, now);
            usable = Some(now);
        }
        tokio::time::sleep(POLL).await;
    }
    mini.0.passthrough.running.store(false, Ordering::Relaxed);
}

fn pointer_over(window: &WebviewWindow) -> bool {
    let (Ok(pointer), Ok(position), Ok(size)) = (
        window.cursor_position(),
        window.outer_position(),
        window.outer_size(),
    ) else {
        return false;
    };
    let x = pointer.x - f64::from(position.x);
    let y = pointer.y - f64::from(position.y);
    (0.0..f64::from(size.width)).contains(&x) && (0.0..f64::from(size.height)).contains(&y)
}
