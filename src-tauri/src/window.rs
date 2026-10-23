//! Native window effects: vibrancy or Liquid Glass on macOS, Mica or Acrylic on Windows,
//! nothing on Linux.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow, WebviewWindowBuilder};

use crate::mini;
use crate::settings::{SettingsStore, WindowMaterial};

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
pub fn create_from_config(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == label)
        .cloned()
        .ok_or_else(|| tauri::Error::WindowNotFound)?;
    config.transparent = cfg!(not(target_os = "linux"));
    WebviewWindowBuilder::from_config(app, &config)?.build()
}

/// What a window's system backdrop should be.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backdrop {
    pub material: WindowMaterial,
    /// Corner radius (points) for a borderless window that draws its own rounded shape.
    pub radius: f64,
    /// Below 1 the backdrop fades while the page stays as it is (macOS); elsewhere the
    /// window goes without one and the page tints itself.
    pub alpha: f64,
}

impl Backdrop {
    /// A normal window: square corners (the system rounds them), fully there.
    pub fn plain(material: WindowMaterial) -> Self {
        Self {
            material,
            radius: 0.0,
            alpha: 1.0,
        }
    }

    /// The backdrop the settings give the window labelled `label`.
    pub fn for_window(app: &AppHandle, label: &str) -> Self {
        let settings = app.state::<SettingsStore>().get();
        let material = settings.appearance.window_material;
        if label == mini::LABEL {
            mini::backdrop(material, &settings.mini_player)
        } else {
            Self::plain(material)
        }
    }
}

/// Effects applied per window label, so the frontend can paint an opaque base where needed.
#[derive(Default)]
pub struct WindowEffectsState(Mutex<HashMap<String, Vibrancy>>);

impl WindowEffectsState {
    /// Applies a system backdrop; must run on the main thread (AppKit).
    pub fn apply(&self, window: &WebviewWindow, backdrop: Backdrop) {
        let vibrancy = apply_backdrop(window, backdrop);
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

    pub fn effects(&self, label: &str) -> WindowEffects {
        WindowEffects {
            platform: std::env::consts::OS,
            vibrancy: self.get(label),
            liquid_glass_supported: liquid_glass_supported(),
        }
    }
}

/// Re-applies the backdrop of every window after the settings changed, then tells each window
/// what it got (Liquid Glass falls back to vibrancy before macOS 26).
pub fn reapply_all(app: &AppHandle) {
    for label in app.webview_windows().into_keys() {
        reapply(app, &label);
    }
}

/// Re-applies one window's backdrop and tells it what it got.
pub fn reapply(app: &AppHandle, label: &str) {
    let handle = app.clone();
    let label = label.to_owned();
    // AppKit views may only be touched on the main thread; commands run on a worker thread.
    let result = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(&label) else {
            return;
        };
        let state = handle.state::<WindowEffectsState>();
        state.apply(&window, Backdrop::for_window(&handle, &label));
        if let Err(err) = handle.emit_to(label.as_str(), EFFECTS_CHANGED, state.effects(&label)) {
            log::warn!("cannot notify window '{label}' about its backdrop: {err}");
        }
    });
    if let Err(err) = result {
        log::warn!("cannot re-apply window backdrops: {err}");
    }
}

#[cfg(target_os = "macos")]
fn apply_backdrop(window: &WebviewWindow, backdrop: Backdrop) -> Vibrancy {
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

    // Liquid Glass moves the page into its own view, so fading it would fade the page too:
    // a faded backdrop is always vibrancy.
    if backdrop.material == WindowMaterial::LiquidGlass && backdrop.alpha >= 1.0 {
        let options =
            LiquidGlassOptions::new(NSGlassEffectViewStyle::Regular).radius(backdrop.radius);
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
        (backdrop.radius > 0.0).then_some(backdrop.radius),
    ) {
        Ok(()) => {
            mini::platform::set_backdrop_alpha(window, backdrop.alpha);
            Vibrancy::Macos
        }
        Err(err) => {
            log::warn!("macOS vibrancy unavailable: {err}");
            Vibrancy::None
        }
    }
}

#[cfg(target_os = "windows")]
fn apply_backdrop(window: &WebviewWindow, backdrop: Backdrop) -> Vibrancy {
    let _ = window_vibrancy::clear_mica(window);
    let _ = window_vibrancy::clear_acrylic(window);
    // Neither material fades: a see-through window goes without one, the page tints itself.
    if backdrop.alpha < 1.0 {
        return Vibrancy::None;
    }
    // Mica needs Windows 11 and a normal frame; Acrylic covers Windows 10 and borderless
    // windows. `None` follows the window theme.
    if backdrop.radius <= 0.0 && window_vibrancy::apply_mica(window, None).is_ok() {
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
fn apply_backdrop(_window: &WebviewWindow, _backdrop: Backdrop) -> Vibrancy {
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
