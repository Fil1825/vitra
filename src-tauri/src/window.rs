//! Native window effects: vibrancy on macOS, Mica or Acrylic on Windows, nothing on Linux.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{App, State, WebviewWindow, WebviewWindowBuilder};

/// Which system backdrop is behind the (transparent) webview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
// Each platform constructs only its own variants.
#[allow(dead_code)]
pub enum Vibrancy {
    Macos,
    Mica,
    Acrylic,
    None,
}

/// Creates a window declared in `tauri.conf.json` with `"create": false`.
///
/// Windows are transparent so the system backdrop shows through, except on Linux: there is no
/// vibrancy to show, and without a compositor a transparent window renders black.
pub fn create_from_config(app: &App, label: &str) -> tauri::Result<WebviewWindow> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    config.transparent = cfg!(not(target_os = "linux"));
    WebviewWindowBuilder::from_config(app.handle(), &config)?.build()
}

/// Effects applied per window label, so the frontend can paint an opaque base where needed.
#[derive(Default)]
pub struct WindowEffectsState(Mutex<HashMap<String, Vibrancy>>);

impl WindowEffectsState {
    pub fn apply(&self, window: &WebviewWindow) {
        let vibrancy = apply_vibrancy(window);
        log::info!("window '{}': vibrancy {vibrancy:?}", window.label());
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(window.label().to_owned(), vibrancy);
    }

    fn get(&self, label: &str) -> Vibrancy {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(label)
            .copied()
            .unwrap_or(Vibrancy::None)
    }
}

fn apply_vibrancy(window: &WebviewWindow) -> Vibrancy {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};
        // `Active` keeps the effect when the window loses focus instead of turning flat grey.
        match window_vibrancy::apply_vibrancy(
            window,
            NSVisualEffectMaterial::UnderWindowBackground,
            Some(NSVisualEffectState::Active),
            None,
        ) {
            Ok(()) => return Vibrancy::Macos,
            Err(err) => log::warn!("macOS vibrancy unavailable: {err}"),
        }
    }
    #[cfg(target_os = "windows")]
    {
        // Mica needs Windows 11; Acrylic covers Windows 10. `None` follows the window theme.
        if window_vibrancy::apply_mica(window, None).is_ok() {
            return Vibrancy::Mica;
        }
        match window_vibrancy::apply_acrylic(window, Some((18, 18, 18, 125))) {
            Ok(()) => return Vibrancy::Acrylic,
            Err(err) => log::warn!("Windows backdrop unavailable: {err}"),
        }
    }
    let _ = window;
    Vibrancy::None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEffects {
    platform: &'static str,
    vibrancy: Vibrancy,
}

/// Platform and backdrop of the calling window. The platform is only used for chrome layout
/// (e.g. room for the macOS traffic lights), never to guess rendering features.
#[tauri::command]
pub fn window_effects(
    window: WebviewWindow,
    state: State<'_, WindowEffectsState>,
) -> WindowEffects {
    WindowEffects {
        platform: std::env::consts::OS,
        vibrancy: state.get(window.label()),
    }
}
