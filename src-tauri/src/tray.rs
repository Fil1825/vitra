//! The icon in the menu bar (macOS) or the notification area (Windows, Linux): what plays,
//! the transport buttons, the windows and Quit. It follows the player from Rust, so it works
//! whichever window is open, and the setting `system.trayIcon` shows or hides it.
//!
//! The menu's words come from the main window (`tray_labels`), where the translations live;
//! English stands in until it has loaded.

use std::sync::{Arc, Mutex};

use serde::Deserialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State};

use crate::error::AppError;
use crate::mini::MiniPlayer;
use crate::player::{Player, PlayerState, Status};

const ID: &str = "vitra";
/// Menu ids, apart from the app's other menus (every menu event reaches every handler).
const NOW: &str = "tray:now";
const TOGGLE: &str = "tray:toggle";
const NEXT: &str = "tray:next";
const PREVIOUS: &str = "tray:previous";
const SHOW: &str = "tray:show";
const MINI: &str = "tray:mini";
const QUIT: &str = "tray:quit";

/// A long title would stretch the menu across the screen.
const MAX_CHARS: usize = 48;

/// The menu's words, in the app's language.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Labels {
    pub not_playing: String,
    pub play: String,
    pub pause: String,
    pub next: String,
    pub previous: String,
    pub show: String,
    pub mini_player: String,
    pub quit: String,
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            not_playing: "Not Playing".into(),
            play: "Play".into(),
            pause: "Pause".into(),
            next: "Next".into(),
            previous: "Previous".into(),
            show: "Show Vitra".into(),
            mini_player: "Mini Player".into(),
            quit: "Quit Vitra".into(),
        }
    }
}

/// What the menu shows about the player.
#[derive(Debug, Clone, Default, PartialEq)]
struct View {
    /// "Title — Artist", or `None` when nothing is loaded.
    now: Option<String>,
    playing: bool,
}

impl View {
    fn of(state: &PlayerState) -> Self {
        let now = state.track.as_ref().map(|track| {
            let metadata = &track.metadata;
            let artist = if metadata.artists.is_empty() {
                metadata.artist.clone().unwrap_or_default()
            } else {
                metadata.artists.join(", ")
            };
            let text = if artist.is_empty() {
                track.title.clone()
            } else {
                format!("{} — {artist}", track.title)
            };
            shorten(&text, MAX_CHARS)
        });
        Self {
            now,
            playing: matches!(state.status, Status::Playing | Status::Loading),
        }
    }
}

/// `text` cut to `max` characters with an ellipsis.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

struct Items {
    now: MenuItem<tauri::Wry>,
    toggle: MenuItem<tauri::Wry>,
    next: MenuItem<tauri::Wry>,
    previous: MenuItem<tauri::Wry>,
    show: MenuItem<tauri::Wry>,
    mini: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

struct Shown {
    labels: Labels,
    view: View,
}

struct Inner {
    app: AppHandle,
    icon: Option<TrayIcon>,
    items: Option<Items>,
    shown: Mutex<Shown>,
}

#[derive(Clone)]
pub struct Tray(Arc<Inner>);

impl Tray {
    /// Puts the icon up (hidden if the setting is off). On the main thread, at setup: a
    /// Linux system without the AppIndicator library has no tray, and says so by panicking.
    pub fn new(app: &AppHandle, visible: bool) -> Self {
        let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(app)));
        let (icon, items) = match built {
            Ok(Ok((icon, items))) => (Some(icon), Some(items)),
            Ok(Err(err)) => {
                log::warn!("no tray icon: {err}");
                (None, None)
            }
            Err(_) => {
                log::warn!("no tray icon: the system has no AppIndicator library");
                (None, None)
            }
        };
        let tray = Self(Arc::new(Inner {
            app: app.clone(),
            icon,
            items,
            shown: Mutex::new(Shown {
                labels: Labels::default(),
                view: View::default(),
            }),
        }));
        tray.set_visible(visible);
        tray
    }

    fn available(&self) -> bool {
        self.0.icon.is_some()
    }

    pub fn set_visible(&self, visible: bool) {
        if let Some(icon) = &self.0.icon
            && let Err(err) = icon.set_visible(visible)
        {
            log::warn!("cannot show or hide the tray icon: {err}");
        }
    }

    /// Follows the player: the track and Play or Pause.
    pub fn update(&self, state: &PlayerState) {
        let view = View::of(state);
        let changed = {
            let mut shown = lock(&self.0.shown);
            let changed = shown.view != view;
            shown.view = view;
            changed
        };
        if changed {
            self.render();
        }
    }

    fn set_labels(&self, labels: Labels) {
        let changed = {
            let mut shown = lock(&self.0.shown);
            let changed = shown.labels != labels;
            shown.labels = labels;
            changed
        };
        if changed {
            self.render();
        }
    }

    /// Writes the texts on the main thread, without waiting for it.
    fn render(&self) {
        let tray = self.clone();
        let rendered = self.0.app.run_on_main_thread(move || {
            let Some(items) = &tray.0.items else {
                return;
            };
            let (labels, view) = {
                let shown = lock(&tray.0.shown);
                (shown.labels.clone(), shown.view.clone())
            };
            let texts = [
                (
                    &items.now,
                    view.now.as_deref().unwrap_or(&labels.not_playing),
                ),
                (
                    &items.toggle,
                    if view.playing {
                        &labels.pause
                    } else {
                        &labels.play
                    },
                ),
                (&items.next, &labels.next),
                (&items.previous, &labels.previous),
                (&items.show, &labels.show),
                (&items.mini, &labels.mini_player),
                (&items.quit, &labels.quit),
            ];
            for (item, text) in texts {
                if let Err(err) = item.set_text(text) {
                    log::warn!("cannot change the tray menu: {err}");
                }
            }
            let loaded = view.now.is_some();
            for item in [&items.toggle, &items.next, &items.previous] {
                let _ = item.set_enabled(loaded);
            }
        });
        if let Err(err) = rendered {
            log::warn!("cannot change the tray menu: {err}");
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn build(app: &AppHandle) -> tauri::Result<(TrayIcon, Items)> {
    let labels = Labels::default();
    let item = |id: &str, text: &str, enabled: bool| {
        MenuItem::with_id(app, id, text, enabled, None::<&str>)
    };
    let items = Items {
        now: item(NOW, &labels.not_playing, false)?,
        toggle: item(TOGGLE, &labels.play, false)?,
        next: item(NEXT, &labels.next, false)?,
        previous: item(PREVIOUS, &labels.previous, false)?,
        show: item(SHOW, &labels.show, true)?,
        mini: item(MINI, &labels.mini_player, true)?,
        quit: item(QUIT, &labels.quit, true)?,
    };
    let menu = Menu::with_items(
        app,
        &[
            &items.now,
            &PredefinedMenuItem::separator(app)?,
            &items.toggle,
            &items.next,
            &items.previous,
            &PredefinedMenuItem::separator(app)?,
            &items.show,
            &items.mini,
            &PredefinedMenuItem::separator(app)?,
            &items.quit,
        ],
    )?;
    let builder = TrayIconBuilder::with_id(ID)
        .menu(&menu)
        .tooltip("Vitra")
        // On Windows a click opens the window and the right button the menu, as there.
        .show_menu_on_left_click(!cfg!(target_os = "windows"))
        .on_menu_event(menu_event)
        .on_tray_icon_event(icon_event);
    // A template image on macOS (the system tints it for the menu bar), the app icon elsewhere.
    let builder = if cfg!(target_os = "macos") {
        builder
            .icon(tauri::include_image!("icons/tray-template.png"))
            .icon_as_template(true)
    } else {
        match app.default_window_icon() {
            Some(icon) => builder.icon(Image::new_owned(
                icon.rgba().to_vec(),
                icon.width(),
                icon.height(),
            )),
            None => builder.icon(tauri::include_image!("icons/32x32.png")),
        }
    };
    let icon = builder.build(app)?;
    Ok((icon, items))
}

fn menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    match id {
        QUIT => app.exit(0),
        SHOW => show_main(app),
        MINI => {
            let Some(mini) = app.try_state::<MiniPlayer>().map(|m| m.inner().clone()) else {
                return;
            };
            tauri::async_runtime::spawn(async move {
                if let Err(err) = mini.toggle().await {
                    log::warn!("cannot open the mini player: {err}");
                }
            });
        }
        TOGGLE | NEXT | PREVIOUS => {
            let Some(player) = app.try_state::<Player>().map(|p| p.inner().clone()) else {
                return;
            };
            let id = id.to_owned();
            tauri::async_runtime::spawn(async move {
                let result = match id.as_str() {
                    TOGGLE => player.toggle().await,
                    NEXT => player.next().await,
                    _ => player.previous().await,
                };
                if let Err(err) = result {
                    log::info!("tray: {id} failed: {err}");
                }
            });
        }
        _ => {}
    }
}

fn icon_event(icon: &TrayIcon, event: TrayIconEvent) {
    if cfg!(target_os = "windows")
        && let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
    {
        show_main(icon.app_handle());
    }
}

/// The main window, in front (the mini player steps aside if it stands in for it).
fn show_main(app: &AppHandle) {
    match app.try_state::<MiniPlayer>() {
        Some(mini) => {
            if let Err(err) = mini.switch_to_main() {
                log::warn!("cannot show the main window: {err}");
            }
        }
        None => {
            if let Some(window) = app.get_webview_window(crate::mini::MAIN) {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
    }
}

/// The menu's words from the main window, in the app's language.
#[tauri::command]
pub fn tray_labels(tray: State<'_, Tray>, labels: Labels) -> Result<(), AppError> {
    tray.set_labels(labels);
    Ok(())
}

/// Whether the system has somewhere to put the icon (a Linux desktop may not).
#[tauri::command]
pub fn tray_available(tray: State<'_, Tray>) -> bool {
    tray.available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_titles_are_cut() {
        assert_eq!(shorten("Song", 10), "Song");
        assert_eq!(shorten("A very long title", 8), "A very…");
        assert_eq!(shorten("Песня очень длинная", 7), "Песня…");
    }
}
