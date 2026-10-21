//! Session restore: the queue, the position, volume, mute and speed survive a restart.
//!
//! The queue (possibly thousands of items) and the small playback values are stored under
//! separate keys, so the position can be saved every few seconds without rewriting the queue.

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::queue::Queue;
use super::state::Status;
use super::{Player, lock};

const QUEUE_KEY: &str = "player.queue";
const PLAYBACK_KEY: &str = "player.playback";
const INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Playback {
    position: f64,
    volume: f64,
    muted: bool,
    speed: f64,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            position: 0.0,
            volume: 80.0,
            muted: false,
            speed: 1.0,
        }
    }
}

impl Player {
    /// Loads the last session: the queue's current item comes back paused where it was.
    pub(super) fn restore_session(&self) {
        let Some(library) = self.library() else {
            return;
        };
        let read = |key: &str| {
            library
                .with(|db| db.session_get(key))
                .inspect_err(|err| log::warn!("cannot read the session: {err}"))
                .ok()
                .flatten()
        };
        let playback = read(PLAYBACK_KEY)
            .and_then(|json| {
                serde_json::from_str::<Playback>(&json)
                    .inspect_err(|err| log::warn!("ignoring a damaged session: {err}"))
                    .ok()
            })
            .unwrap_or_default();
        {
            let mut state = lock(&self.0.state);
            state.volume = playback.volume.clamp(0.0, 100.0);
            state.muted = playback.muted;
            state.speed = playback.speed.clamp(0.25, 4.0);
        }

        let Some(mut queue) = read(QUEUE_KEY).and_then(|json| {
            serde_json::from_str::<Queue>(&json)
                .inspect_err(|err| log::warn!("ignoring a damaged queue: {err}"))
                .ok()
        }) else {
            return;
        };
        queue.sanitize();
        let current = queue.current().cloned();
        *lock(&self.0.queue) = queue;
        // A file deleted since is shown in the queue but not loaded (it would only fail).
        if let Some(item) = current.filter(|item| Path::new(&item.entry.path).is_file()) {
            self.begin_track(&item);
            let mut state = lock(&self.0.state);
            state.status = Status::Paused;
            state.paused = true;
            state.position = playback.position.max(0.0);
        }
    }

    /// Writes what changed since the last save (everything with `force`). Blocking.
    pub(super) fn save_session(&self, force: bool) {
        let Some(library) = self.library() else {
            return;
        };
        if self.0.queue_dirty.swap(false, Ordering::Relaxed) || force {
            let json = serde_json::to_string(&*lock(&self.0.queue));
            if let Ok(json) = json
                && let Err(err) = library.with(|db| db.session_set(QUEUE_KEY, &json))
            {
                log::warn!("cannot save the queue: {err}");
            }
        }

        let playback = {
            let state = lock(&self.0.state);
            Playback {
                // Whole seconds: a paused track doesn't cause writes, and a second is close
                // enough to resume from.
                position: if state.status == Status::Idle {
                    0.0
                } else {
                    state.position.floor()
                },
                volume: state.volume,
                muted: state.muted,
                speed: state.speed,
            }
        };
        let Ok(json) = serde_json::to_string(&playback) else {
            return;
        };
        let mut saved = lock(&self.0.saved_playback);
        if force || *saved != json {
            match library.with(|db| db.session_set(PLAYBACK_KEY, &json)) {
                Ok(()) => *saved = json,
                Err(err) => log::warn!("cannot save the session: {err}"),
            }
        }
    }
}

/// Saves the session every few seconds while the app runs.
pub(super) async fn autosave(player: Player) {
    loop {
        tokio::time::sleep(INTERVAL).await;
        let player = player.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || player.save_session(false)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn playback_defaults_fill_missing_fields() {
        let playback: Playback = serde_json::from_str(r#"{"volume": 35}"#).unwrap();
        assert_eq!(
            playback,
            Playback {
                volume: 35.0,
                ..Playback::default()
            }
        );
    }
}
