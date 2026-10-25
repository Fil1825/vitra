//! Shortcuts that work while another app is in front (`settings.globalShortcuts`: an action
//! id with its combination, none until the user sets one). Settings records them like the
//! window's own (`src/shortcuts`); here they are registered with the system and run in Rust,
//! so they work whichever window is open, or none.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::mini::MiniPlayer;
use crate::player::{Player, Repeat};
use crate::shortcuts::GlobalShortcuts;

const SEEK_STEP: f64 = 5.0;
const LONG_SEEK_STEP: f64 = 15.0;
const VOLUME_STEP: f64 = 5.0;

/// What a shortcut in every app can do: playback and the mini player (the same ids as the
/// window's actions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    PlayPause,
    Next,
    Previous,
    SeekForward,
    SeekBack,
    SeekForwardLong,
    SeekBackLong,
    VolumeUp,
    VolumeDown,
    Mute,
    Shuffle,
    Repeat,
    MiniPlayer,
}

impl Action {
    fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "play-pause" => Self::PlayPause,
            "next" => Self::Next,
            "previous" => Self::Previous,
            "seek-forward" => Self::SeekForward,
            "seek-back" => Self::SeekBack,
            "seek-forward-long" => Self::SeekForwardLong,
            "seek-back-long" => Self::SeekBackLong,
            "volume-up" => Self::VolumeUp,
            "volume-down" => Self::VolumeDown,
            "mute" => Self::Mute,
            "shuffle" => Self::Shuffle,
            "repeat" => Self::Repeat,
            "mini-player" => Self::MiniPlayer,
            _ => return None,
        })
    }
}

/// Keys that may be a shortcut in every app on their own; anything else needs ⌘, ⌃ or ⌥
/// (Ctrl or Alt), or it would take a key away from typing in every app.
fn alone_is_fine(code: Code) -> bool {
    let name = code.to_string();
    let function_key = name
        .strip_prefix('F')
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
    function_key || name.starts_with("Media") || name.starts_with("AudioVolume")
}

/// The system's shortcut for a combination as Settings writes it (`Mod+Alt+KeyP`: `Mod` is
/// ⌘ on macOS and Ctrl elsewhere, `Ctrl` the Control key on macOS).
fn shortcut(combo: &str, mac: bool) -> Option<Shortcut> {
    let mut parts: Vec<&str> = combo.split('+').collect();
    let code = Code::from_str(parts.pop()?).ok()?;
    let mut modifiers = Modifiers::empty();
    for part in parts {
        modifiers |= match part {
            "Mod" if mac => Modifiers::SUPER,
            "Mod" | "Ctrl" => Modifiers::CONTROL,
            "Alt" => Modifiers::ALT,
            "Shift" => Modifiers::SHIFT,
            _ => return None,
        };
    }
    let chord = modifiers.intersects(Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT);
    (chord || alone_is_fine(code)).then(|| Shortcut::new(Some(modifiers), code))
}

/// The shortcuts registered with the system, and those it refused.
#[derive(Default)]
pub struct GlobalKeys {
    bound: Mutex<HashMap<u32, Action>>,
    /// Action ids whose combination couldn't be registered (another app has it).
    failed: Mutex<Vec<String>>,
}

impl GlobalKeys {
    /// Registers `shortcuts` in place of the ones before (on the main thread, or it waits
    /// for it).
    pub fn apply(&self, app: &AppHandle, shortcuts: &GlobalShortcuts) {
        let manager = app.global_shortcut();
        if let Err(err) = manager.unregister_all() {
            log::warn!("cannot remove shortcuts in every app: {err}");
        }
        let mac = cfg!(target_os = "macos");
        let mut bound = HashMap::new();
        let mut failed = Vec::new();
        for (id, combo) in shortcuts {
            let Some(action) = Action::from_id(id) else {
                continue;
            };
            let registered = shortcut(combo, mac)
                .ok_or_else(|| "not a shortcut for every app".to_owned())
                .and_then(|key| {
                    manager
                        .register(key)
                        .map(|()| key)
                        .map_err(|err| err.to_string())
                });
            match registered {
                Ok(key) => {
                    bound.insert(key.id(), action);
                }
                Err(err) => {
                    log::info!("shortcut {combo} for {id} in every app: {err}");
                    failed.push(id.clone());
                }
            }
        }
        *lock(&self.bound) = bound;
        *lock(&self.failed) = failed;
    }

    /// The plugin's handler: a registered shortcut was pressed (or released).
    pub fn handle(app: &AppHandle, key: &Shortcut, event: ShortcutEvent) {
        let Some(keys) = app.try_state::<GlobalKeys>() else {
            return;
        };
        let Some(action) = lock(&keys.bound).get(&key.id()).copied() else {
            return;
        };
        // Once per press: the systems don't repeat a held shortcut (Windows is told not to).
        if event.state == ShortcutState::Pressed {
            run(app, action);
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn run(app: &AppHandle, action: Action) {
    if action == Action::MiniPlayer {
        let Some(mini) = app.try_state::<MiniPlayer>().map(|m| m.inner().clone()) else {
            return;
        };
        tauri::async_runtime::spawn(async move {
            if let Err(err) = mini.toggle().await {
                log::warn!("cannot open the mini player: {err}");
            }
        });
        return;
    }
    let Some(player) = app.try_state::<Player>().map(|p| p.inner().clone()) else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let state = player.state();
        let seek = |by: f64| {
            let player = player.clone();
            async move {
                if state.duration.is_none() {
                    return Ok(());
                }
                player.seek((state.position + by).max(0.0)).await
            }
        };
        let result = match action {
            Action::PlayPause if state.track.is_some() => player.toggle().await,
            Action::PlayPause => Ok(()),
            Action::Next => player.next().await,
            Action::Previous => player.previous().await,
            Action::SeekForward => seek(SEEK_STEP).await,
            Action::SeekBack => seek(-SEEK_STEP).await,
            Action::SeekForwardLong => seek(LONG_SEEK_STEP).await,
            Action::SeekBackLong => seek(-LONG_SEEK_STEP).await,
            Action::VolumeUp => player.set_volume(state.volume + VOLUME_STEP).await,
            Action::VolumeDown => player.set_volume(state.volume - VOLUME_STEP).await,
            Action::Mute => player.set_muted(!state.muted).await,
            Action::Shuffle => player.set_shuffle(!player.queue().shuffle).await,
            Action::Repeat => {
                let next = match player.queue().repeat {
                    Repeat::Off => Repeat::All,
                    Repeat::All => Repeat::One,
                    Repeat::One => Repeat::Off,
                };
                player.set_repeat(next).await
            }
            Action::MiniPlayer => Ok(()),
        };
        if let Err(err) = result {
            log::info!("shortcut {action:?} failed: {err}");
        }
    });
}

/// Actions whose shortcut in every app the system refused (another app has it).
#[tauri::command]
pub fn global_shortcuts_failed(keys: State<'_, GlobalKeys>) -> Vec<String> {
    lock(&keys.failed).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_become_the_systems_shortcuts() {
        let mac = shortcut("Mod+Alt+KeyP", true).unwrap();
        assert_eq!(mac.mods, Modifiers::SUPER | Modifiers::ALT);
        assert_eq!(mac.key, Code::KeyP);
        let other = shortcut("Mod+Shift+ArrowRight", false).unwrap();
        assert_eq!(other.mods, Modifiers::CONTROL | Modifiers::SHIFT);
        assert_eq!(other.key, Code::ArrowRight);
        assert_eq!(
            shortcut("Ctrl+Mod+Space", true).unwrap().mods,
            Modifiers::CONTROL | Modifiers::SUPER
        );
    }

    #[test]
    fn plain_keys_stay_with_typing() {
        assert_eq!(shortcut("KeyK", true), None);
        assert_eq!(shortcut("Shift+KeyK", false), None);
        assert_eq!(shortcut("Space", false), None);
        // Function and media keys type nothing.
        assert!(shortcut("F13", true).is_some());
        assert!(shortcut("Shift+F8", false).is_some());
        assert!(shortcut("MediaPlayPause", false).is_some());
        assert_eq!(shortcut("Fn", true), None);
    }

    #[test]
    fn nonsense_is_refused() {
        assert_eq!(shortcut("Mod+Hyper+KeyK", true), None);
        assert_eq!(shortcut("Mod+NoSuchKey", true), None);
        assert_eq!(shortcut("", true), None);
    }

    #[test]
    fn actions_are_the_windows_ids() {
        assert_eq!(Action::from_id("play-pause"), Some(Action::PlayPause));
        assert_eq!(Action::from_id("mini-player"), Some(Action::MiniPlayer));
        // Window-only actions have no meaning in other apps.
        assert_eq!(Action::from_id("search"), None);
    }
}
