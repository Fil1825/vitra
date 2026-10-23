//! The mini player: a second, small window (`mini.html`) with three modes. Its content mirrors
//! the player through the same events as the main window; this module owns the window itself:
//! showing it in place of the main window, its size per mode, snapping to the screen's edges,
//! staying on top (over full-screen apps on macOS too), click-through with ⌥/Alt, and fading
//! its backdrop.

mod commands;
mod geometry;
mod passthrough;
pub(crate) mod platform;

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Size as TauriSize, WebviewWindow,
    WindowEvent,
};

pub use commands::*;
pub use geometry::{Mode, SizePreset};
use geometry::{Rect, Saved, Size};

use crate::error::AppError;
use crate::settings::{MiniPlayerSettings, SettingsStore, WindowMaterial};
use crate::window::{self, Backdrop};

/// The mini player's window label (`tauri.conf.json`).
pub const LABEL: &str = "mini";
/// The main window's label.
pub const MAIN: &str = "main";
/// Corner radius (points) of the mini player's backdrop; the page rounds its glass to match.
pub const RADIUS: f64 = 14.0;
/// Within this distance (logical px) of a screen edge, a dropped window snaps to it.
const SNAP_DISTANCE: f64 = 16.0;
/// Moving or resizing has stopped when nothing happened for this long.
const SETTLE: Duration = Duration::from_millis(250);

/// The mini player's backdrop: rounded, and faded by its opacity setting.
pub fn backdrop(material: WindowMaterial, settings: &MiniPlayerSettings) -> Backdrop {
    Backdrop {
        material,
        radius: RADIUS,
        alpha: settings.opacity,
    }
}

#[derive(Clone)]
pub struct MiniPlayer(Arc<Inner>);

/// What the window is settling from.
#[derive(Debug, Clone, Copy)]
enum Change {
    Move,
    Resize,
}

struct Inner {
    app: AppHandle,
    /// `<app config>/mini-player.json`.
    path: std::path::PathBuf,
    saved: Mutex<Saved>,
    /// Bumped by every move and resize: the work after them waits until the window settles.
    moves: AtomicU64,
    resizes: AtomicU64,
    passthrough: passthrough::Passthrough,
    /// The activation policy last set on macOS (`true`: no Dock icon).
    #[cfg(target_os = "macos")]
    accessory: std::sync::atomic::AtomicBool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn rect(position: PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>) -> Rect {
    Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    }
}

impl MiniPlayer {
    pub fn new(app: AppHandle, config_dir: &Path) -> Self {
        let path = config_dir.join("mini-player.json");
        let saved = Saved::load(&path);
        Self(Arc::new(Inner {
            app,
            path,
            saved: Mutex::new(saved),
            moves: AtomicU64::new(0),
            resizes: AtomicU64::new(0),
            passthrough: passthrough::Passthrough::default(),
            #[cfg(target_os = "macos")]
            accessory: std::sync::atomic::AtomicBool::new(false),
        }))
    }

    /// It was open when the app last quit (and should open again).
    pub fn was_open(&self) -> bool {
        lock(&self.0.saved).open
    }

    fn settings(&self) -> MiniPlayerSettings {
        self.0
            .app
            .try_state::<SettingsStore>()
            .map(|store| store.get().mini_player)
            .unwrap_or_default()
    }

    fn window(&self) -> Option<WebviewWindow> {
        self.0.app.get_webview_window(LABEL)
    }

    fn main(&self) -> Option<WebviewWindow> {
        self.0.app.get_webview_window(MAIN)
    }

    fn visible(window: Option<&WebviewWindow>) -> bool {
        window.is_some_and(|window| window.is_visible().unwrap_or(false))
    }

    /// Shows the mini player (creating its window the first time), and hides the main window
    /// if the settings say so.
    pub async fn show(&self) -> Result<(), AppError> {
        let window = match self.ensure_window() {
            Ok(window) => window,
            Err(err) => {
                // Never leave the app without a window.
                self.show_main();
                return Err(err);
            }
        };
        let settings = self.settings();
        if !window.is_visible()? {
            self.place(&window, &settings)?;
        }
        window.show()?;
        window.set_focus()?;
        if settings.hide_main_window
            && let Some(main) = self.main()
        {
            main.hide()?;
        }
        self.set_open(true);
        if settings.click_through {
            self.0.passthrough.start(self);
        }
        self.update_activation_policy();
        Ok(())
    }

    /// Hides the mini player; the main window comes back if it was hidden.
    pub fn hide(&self) -> Result<(), AppError> {
        if let Some(window) = self.window() {
            window.hide()?;
        }
        self.set_open(false);
        if !Self::visible(self.main().as_ref()) {
            self.show_main();
        }
        self.update_activation_policy();
        Ok(())
    }

    pub async fn toggle(&self) -> Result<(), AppError> {
        if Self::visible(self.window().as_ref()) {
            self.hide()
        } else {
            self.show().await
        }
    }

    /// Back to the main window ("expand" in the mini player, the Dock icon): the mini player
    /// goes away if it stands in for the main window.
    pub fn switch_to_main(&self) -> Result<(), AppError> {
        if self.settings().hide_main_window && Self::visible(self.window().as_ref()) {
            return self.hide();
        }
        self.show_main();
        self.update_activation_policy();
        Ok(())
    }

    fn show_main(&self) {
        let Some(main) = self.main() else {
            return;
        };
        let shown = main
            .show()
            .and_then(|()| main.unminimize())
            .and_then(|()| main.set_focus());
        if let Err(err) = shown {
            log::warn!("cannot show the main window: {err}");
        }
    }

    /// The main window is being closed: with the mini player open the app goes on in it, else
    /// the app quits as before (a hidden mini player window would keep it running).
    pub fn main_close_requested(&self, api: &tauri::CloseRequestApi) {
        let Some(window) = self.window() else {
            return;
        };
        if Self::visible(Some(&window)) {
            api.prevent_close();
            if let Some(main) = self.main()
                && let Err(err) = main.hide()
            {
                log::warn!("cannot hide the main window: {err}");
            }
            self.update_activation_policy();
        } else if let Err(err) = window.destroy() {
            log::warn!("cannot close the mini player: {err}");
        }
    }

    fn ensure_window(&self) -> Result<WebviewWindow, AppError> {
        if let Some(window) = self.window() {
            return Ok(window);
        }
        let window = window::create_from_config(&self.0.app, LABEL)?;
        self.configure(&window, &self.settings());
        let mini = self.clone();
        window.on_window_event(move |event| mini.window_event(event));
        window::reapply(&self.0.app, LABEL);
        Ok(window)
    }

    /// Settings that don't depend on the mode.
    fn configure(&self, window: &WebviewWindow, settings: &MiniPlayerSettings) {
        let on_top = settings.always_on_top;
        let applied = window
            .set_always_on_top(on_top)
            .and_then(|()| window.set_visible_on_all_workspaces(on_top))
            .and_then(|()| window.set_ignore_cursor_events(settings.click_through));
        if let Err(err) = applied {
            log::warn!("cannot set up the mini player window: {err}");
        }
        if on_top {
            let window = window.clone();
            let result = self.0.app.run_on_main_thread(move || {
                platform::join_full_screen_spaces(&window);
            });
            if let Err(err) = result {
                log::warn!("cannot let the mini player into full-screen spaces: {err}");
            }
        }
    }

    /// Sizes the window for its mode and puts it where it was (or near the bottom right).
    fn place(&self, window: &WebviewWindow, settings: &MiniPlayerSettings) -> Result<(), AppError> {
        let logical = self.size_for_mode(window, settings)?;
        // From the size just set: a new window reports its old size for a moment.
        let Some(monitor) = window.primary_monitor()?.or(window.current_monitor()?) else {
            return Ok(());
        };
        let size = tauri::LogicalSize::new(logical.width, logical.height)
            .to_physical::<u32>(monitor.scale_factor());
        let saved = lock(&self.0.saved).position;
        let position = saved.filter(|&(x, y)| {
            settings.remember_position
                && geometry::reachable(rect(PhysicalPosition::new(x, y), size), &work_areas(window))
        });
        let (x, y) = position.unwrap_or_else(|| {
            geometry::default_position(size.width, size.height, work_area(&monitor))
        });
        window.set_position(PhysicalPosition::new(x, y))?;
        Ok(())
    }

    /// Limits and size of the current mode: the size it had, else the preset's. Returns the
    /// size set.
    fn size_for_mode(
        &self,
        window: &WebviewWindow,
        settings: &MiniPlayerSettings,
    ) -> Result<Size, AppError> {
        let mode = settings.mode;
        let (min, max) = mode.limits();
        // Lowering the minimum first lets a smaller mode's size through.
        window.set_min_size(Some(logical(min)))?;
        window.set_max_size(Some(logical(max)))?;
        let remembered = settings
            .remember_position
            .then(|| lock(&self.0.saved).sizes.get(&mode).copied())
            .flatten()
            .map(|size| geometry::clamp_size(mode, size));
        let size = remembered.unwrap_or_else(|| geometry::preset_size(mode, settings.size));
        window.set_size(logical(size))?;
        Ok(size)
    }

    /// Applies changed settings to an open window.
    pub fn settings_changed(&self, old: &MiniPlayerSettings, new: &MiniPlayerSettings) {
        if new.size != old.size {
            // A preset is for every mode; sizes dragged to before give way.
            lock(&self.0.saved).sizes.clear();
            self.save();
        }
        let Some(window) = self.window() else {
            return;
        };
        if new.always_on_top != old.always_on_top || new.click_through != old.click_through {
            self.configure(&window, new);
        }
        if new.click_through {
            self.0.passthrough.start(self);
        } else if old.click_through {
            passthrough::stop(&window);
        }
        if new.opacity != old.opacity {
            window::reapply(&self.0.app, LABEL);
        }
        if new.mode != old.mode || new.size != old.size {
            // The `Resized` that follows keeps it on the screen.
            if let Err(err) = self.size_for_mode(&window, new) {
                log::warn!("cannot resize the mini player: {err}");
            }
        }
        self.update_activation_policy();
    }

    fn window_event(&self, event: &WindowEvent) {
        match event {
            WindowEvent::Moved(_) => self.after_settling(Change::Move, Self::moved),
            WindowEvent::Resized(_) => self.after_settling(Change::Resize, Self::resized),
            // ⌘W or Alt+F4 while it has focus.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if let Err(err) = self.hide() {
                    log::warn!("cannot hide the mini player: {err}");
                }
            }
            _ => {}
        }
    }

    /// Runs `then` once the window has stopped moving (or resizing) for a moment and the
    /// mouse button is up.
    fn after_settling(&self, change: Change, then: fn(&MiniPlayer, &WebviewWindow)) {
        let ticket = self.counter(change).fetch_add(1, Ordering::Relaxed) + 1;
        let mini = self.clone();
        tauri::async_runtime::spawn(async move {
            let counter = mini.counter(change);
            loop {
                tokio::time::sleep(SETTLE).await;
                if counter.load(Ordering::Relaxed) != ticket {
                    return;
                }
                // Still held: the user is dragging but has paused.
                if platform::mouse_down() != Some(true) {
                    break;
                }
            }
            if let Some(window) = mini.window() {
                then(&mini, &window);
            }
        });
    }

    fn counter(&self, change: Change) -> &AtomicU64 {
        match change {
            Change::Move => &self.0.moves,
            Change::Resize => &self.0.resizes,
        }
    }

    fn moved(&self, window: &WebviewWindow) {
        let settings = self.settings();
        if settings.snap_to_edges && self.snap(window) {
            // Moving it makes another `Moved`, which saves the new position.
            return;
        }
        if let Ok(position) = window.outer_position() {
            lock(&self.0.saved).position = Some((position.x, position.y));
            if settings.remember_position {
                self.save();
            }
        }
    }

    /// Moves the window flush against screen edges it was dropped close to.
    fn snap(&self, window: &WebviewWindow) -> bool {
        let (Ok(position), Ok(size), Ok(Some(monitor)), Ok(scale)) = (
            window.outer_position(),
            window.outer_size(),
            window.current_monitor(),
            window.scale_factor(),
        ) else {
            return false;
        };
        // Rounded: a scale factor is a small positive float, the distance a few dozen pixels.
        #[allow(clippy::cast_possible_truncation)]
        let threshold = (SNAP_DISTANCE * scale).round() as i64;
        match geometry::snap(rect(position, size), work_area(&monitor), threshold) {
            Some((x, y)) => window.set_position(PhysicalPosition::new(x, y)).is_ok(),
            None => false,
        }
    }

    fn resized(&self, window: &WebviewWindow) {
        let settings = self.settings();
        let (Ok(size), Ok(scale)) = (window.inner_size(), window.scale_factor()) else {
            return;
        };
        let logical = size.to_logical::<f64>(scale);
        lock(&self.0.saved).sizes.insert(
            settings.mode,
            Size {
                width: logical.width.round(),
                height: logical.height.round(),
            },
        );
        if settings.remember_position {
            self.save();
        }
        self.keep_on_screen(window);
    }

    /// A window that grew past the screen's edge comes back inside.
    fn keep_on_screen(&self, window: &WebviewWindow) {
        let (Ok(position), Ok(size), Ok(Some(monitor))) = (
            window.outer_position(),
            window.outer_size(),
            window.current_monitor(),
        ) else {
            return;
        };
        if let Some((x, y)) = geometry::fit_inside(rect(position, size), work_area(&monitor))
            && let Err(err) = window.set_position(PhysicalPosition::new(x, y))
        {
            log::warn!("cannot move the mini player onto the screen: {err}");
        }
    }

    fn set_open(&self, open: bool) {
        let changed = {
            let mut saved = lock(&self.0.saved);
            std::mem::replace(&mut saved.open, open) != open
        };
        if changed {
            self.save();
        }
    }

    fn save(&self) {
        let saved = lock(&self.0.saved).clone();
        if let Err(err) = saved.save(&self.0.path) {
            log::warn!("cannot save {}: {err}", self.0.path.display());
        }
    }

    /// macOS shows an app's windows over another app's full-screen space only when the app
    /// has no Dock icon: while the mini player is the only window and should float over
    /// full-screen apps, Vitra becomes such an app.
    fn update_activation_policy(&self) {
        #[cfg(target_os = "macos")]
        {
            let settings = self.settings();
            let alone =
                Self::visible(self.window().as_ref()) && !Self::visible(self.main().as_ref());
            let accessory = alone && settings.always_on_top && settings.over_full_screen;
            if self.0.accessory.swap(accessory, Ordering::Relaxed) != accessory {
                let policy = if accessory {
                    tauri::ActivationPolicy::Accessory
                } else {
                    tauri::ActivationPolicy::Regular
                };
                if let Err(err) = self.0.app.set_activation_policy(policy) {
                    log::warn!("cannot change the activation policy: {err}");
                }
            }
        }
    }
}

fn logical(size: Size) -> TauriSize {
    TauriSize::Logical(LogicalSize::new(size.width, size.height))
}

fn work_area(monitor: &tauri::Monitor) -> Rect {
    let area = monitor.work_area();
    rect(area.position, area.size)
}

fn work_areas(window: &WebviewWindow) -> Vec<Rect> {
    window
        .available_monitors()
        .map(|monitors| monitors.iter().map(work_area).collect())
        .unwrap_or_default()
}
