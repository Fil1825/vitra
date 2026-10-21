//! Native window effects: vibrancy or Liquid Glass on macOS, Mica or Acrylic on Windows,
//! nothing on Linux.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{App, AppHandle, Emitter, Manager, State, WebviewWindow, WebviewWindowBuilder};

use crate::settings::WindowMaterial;

/// Emitted to a window with its new [`WindowEffects`] after the material changed.
pub const EFFECTS_CHANGED: &str = "window://effects-changed";

/// Which system backdrop is behind the (transparent) webview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
// Each platform constructs only its own variants.
#[allow(dead_code)]
pub enum Vibrancy {
    Macos,
    LiquidGlass,
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
    /// Applies the system backdrop for `material`; must run on the main thread (AppKit).
    pub fn apply(&self, window: &WebviewWindow, material: WindowMaterial) {
        let vibrancy = apply_backdrop(window, material);
        log::info!("window '{}': backdrop {vibrancy:?}", window.label());
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

    fn effects(&self, label: &str) -> WindowEffects {
        WindowEffects {
            platform: std::env::consts::OS,
            vibrancy: self.get(label),
            liquid_glass_supported: liquid_glass_supported(),
        }
    }
}

/// Re-applies the backdrop of every window after the material setting changed, then tells each
/// window what it got (Liquid Glass falls back to vibrancy before macOS 26).
pub fn reapply_all(app: &AppHandle, material: WindowMaterial) {
    let handle = app.clone();
    // AppKit views may only be touched on the main thread; commands run on a worker thread.
    let result = app.run_on_main_thread(move || {
        let state = handle.state::<WindowEffectsState>();
        for (label, window) in handle.webview_windows() {
            state.apply(&window, material);
            if let Err(err) = handle.emit_to(label.as_str(), EFFECTS_CHANGED, state.effects(&label))
            {
                log::warn!("cannot notify window '{label}' about its backdrop: {err}");
            }
        }
    });
    if let Err(err) = result {
        log::warn!("cannot re-apply window backdrops: {err}");
    }
}

#[cfg(target_os = "macos")]
fn apply_backdrop(window: &WebviewWindow, material: WindowMaterial) -> Vibrancy {
    use window_vibrancy::{
        LiquidGlassOptions, NSGlassEffectViewStyle, NSVisualEffectMaterial, NSVisualEffectState,
    };

    // When switching at runtime, remove the view an earlier call inserted; no-ops otherwise.
    if let Err(err) = window_vibrancy::clear_liquid_glass(window) {
        log::debug!("clearing Liquid Glass: {err}");
    }
    if let Err(err) = window_vibrancy::clear_vibrancy(window) {
        log::debug!("clearing vibrancy: {err}");
    }

    if material == WindowMaterial::LiquidGlass {
        let options = LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular);
        match window_vibrancy::apply_liquid_glass(window, options) {
            Ok(()) => return Vibrancy::LiquidGlass,
            Err(err) => log::warn!("Liquid Glass unavailable, using vibrancy: {err}"),
        }
    }

    // `Active` keeps the effect when the window loses focus instead of turning flat grey.
    match window_vibrancy::apply_vibrancy(
        window,
        NSVisualEffectMaterial::UnderWindowBackground,
        Some(NSVisualEffectState::Active),
        None,
    ) {
        Ok(()) => Vibrancy::Macos,
        Err(err) => {
            log::warn!("macOS vibrancy unavailable: {err}");
            Vibrancy::None
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_backdrop(window: &WebviewWindow, _material: WindowMaterial) -> Vibrancy {
    // Mica needs Windows 11; Acrylic covers Windows 10. `None` follows the window theme.
    if window_vibrancy::apply_mica(window, None).is_ok() {
        return Vibrancy::Mica;
    }
    match window_vibrancy::apply_acrylic(window, Some((18, 18, 18, 125))) {
        Ok(()) => Vibrancy::Acrylic,
        Err(err) => {
            log::warn!("Windows backdrop unavailable: {err}");
            Vibrancy::None
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn apply_backdrop(_window: &WebviewWindow, _material: WindowMaterial) -> Vibrancy {
    Vibrancy::None
}

/// Liquid Glass (`NSGlassEffectView`) exists from macOS 26.
#[cfg(target_os = "macos")]
fn liquid_glass_supported() -> bool {
    use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(NSOperatingSystemVersion {
        majorVersion: 26,
        minorVersion: 0,
        patchVersion: 0,
    })
}

#[cfg(not(target_os = "macos"))]
fn liquid_glass_supported() -> bool {
    false
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowEffects {
    platform: &'static str,
    vibrancy: Vibrancy,
    liquid_glass_supported: bool,
}

/// Platform and backdrop of the calling window. The platform is only used for chrome layout
/// (e.g. room for the macOS traffic lights), never to guess rendering features.
#[tauri::command]
pub fn window_effects(
    window: WebviewWindow,
    state: State<'_, WindowEffectsState>,
) -> WindowEffects {
    state.effects(window.label())
}
