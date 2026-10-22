//! Playback state as the UI sees it, and how mpv events change it.

use media::{Palette, TrackMetadata};
use mpv_ipc::{EndReason, Event};
use serde::Serialize;
use serde_json::Value;

/// Properties observed on every mpv instance; their changes drive [`PlayerState::apply`]
/// (and `metadata`, the stream titles of internet radio, the player).
pub const OBSERVED: &[&str] = &[
    "pause", "time-pos", "duration", "volume", "mute", "speed", "metadata",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Nothing loaded, or the last track ended.
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkInfo {
    /// Served at `artwork://<id>` (full size) and `artwork://<id>-thumb`.
    pub id: String,
    pub palette: Option<Palette>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Changes with every load, so late metadata for an older load can be dropped.
    pub id: u64,
    pub path: String,
    /// The library track, if the file is in the library.
    pub library_id: Option<i64>,
    /// Tag title, or the file name without extension.
    pub title: String,
    pub metadata: TrackMetadata,
    pub artwork: Option<ArtworkInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlayerError {
    /// No mpv binary was found.
    MpvMissing,
    /// mpv could not be started or kept crashing.
    MpvFailed { message: String },
    /// The current file could not be played.
    Playback { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub status: Status,
    pub track: Option<Track>,
    /// Seconds.
    pub position: f64,
    pub duration: Option<f64>,
    /// 0–100.
    pub volume: f64,
    pub muted: bool,
    pub speed: f64,
    pub error: Option<PlayerError>,
    /// mpv's `pause` flag; it is set even while nothing is playing.
    #[serde(skip)]
    pub paused: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: Status::Idle,
            track: None,
            position: 0.0,
            duration: None,
            volume: 80.0,
            muted: false,
            speed: 1.0,
            error: None,
            paused: false,
        }
    }
}

/// What an event changed, so the caller emits only what's needed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    /// Anything besides the position.
    pub state: bool,
    pub position: bool,
}

impl PlayerState {
    /// Applies an mpv event.
    pub fn apply(&mut self, event: &Event) -> Changes {
        let before = self.clone();
        match event {
            Event::PropertyChange { name, data, .. } => self.apply_property(name, data.as_ref()),
            Event::StartFile { .. } => {
                self.status = Status::Loading;
                self.position = 0.0;
                self.error = None;
            }
            Event::FileLoaded => {
                if self.status == Status::Loading {
                    self.status = if self.paused {
                        Status::Paused
                    } else {
                        Status::Playing
                    };
                }
            }
            Event::EndFile { reason, error, .. } => match reason {
                // A replaced file also ends with `stop`; the next `start-file` follows.
                EndReason::Stop | EndReason::Redirect => {}
                EndReason::Error => {
                    self.status = Status::Idle;
                    self.position = 0.0;
                    self.error = Some(PlayerError::Playback {
                        message: error
                            .clone()
                            .unwrap_or_else(|| "cannot play this file".into()),
                    });
                }
                EndReason::Eof | EndReason::Quit | EndReason::Unknown => {
                    self.status = Status::Idle;
                    self.position = 0.0;
                }
            },
            _ => {}
        }

        let position = self.position != before.position;
        let mut rest = self.clone();
        rest.position = before.position;
        Changes {
            state: rest != before,
            position,
        }
    }

    fn apply_property(&mut self, name: &str, data: Option<&Value>) {
        let number = data.and_then(Value::as_f64);
        let flag = data.and_then(Value::as_bool);
        match name {
            "pause" => {
                if let Some(paused) = flag {
                    if matches!(self.status, Status::Playing | Status::Paused) {
                        self.status = if paused {
                            Status::Paused
                        } else {
                            Status::Playing
                        };
                    }
                    self.paused = paused;
                }
            }
            "time-pos" => self.position = number.unwrap_or(0.0).max(0.0),
            "duration" => self.duration = number,
            "volume" => {
                if let Some(volume) = number {
                    self.volume = volume;
                }
            }
            "mute" => {
                if let Some(muted) = flag {
                    self.muted = muted;
                }
            }
            "speed" => {
                if let Some(speed) = number {
                    self.speed = speed;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn property(name: &str, data: Value) -> Event {
        Event::PropertyChange {
            id: 1,
            name: name.into(),
            data: Some(data),
        }
    }

    fn loaded(paused: bool) -> PlayerState {
        let mut state = PlayerState::default();
        state.apply(&property("pause", json!(paused)));
        state.apply(&Event::StartFile {
            playlist_entry_id: Some(1),
        });
        state.apply(&Event::FileLoaded);
        state
    }

    #[test]
    fn loading_then_playing() {
        let mut state = PlayerState::default();
        state.apply(&Event::StartFile {
            playlist_entry_id: Some(1),
        });
        assert_eq!(state.status, Status::Loading);
        state.apply(&Event::FileLoaded);
        assert_eq!(state.status, Status::Playing);
    }

    #[test]
    fn a_file_loaded_while_paused_stays_paused() {
        assert_eq!(loaded(true).status, Status::Paused);
    }

    #[test]
    fn pause_toggles_only_a_loaded_file() {
        let mut idle = PlayerState::default();
        idle.apply(&property("pause", json!(true)));
        assert_eq!(idle.status, Status::Idle);

        let mut state = loaded(false);
        state.apply(&property("pause", json!(true)));
        assert_eq!(state.status, Status::Paused);
        state.apply(&property("pause", json!(false)));
        assert_eq!(state.status, Status::Playing);
    }

    #[test]
    fn position_changes_are_reported_separately() {
        let mut state = loaded(false);
        let changes = state.apply(&property("time-pos", json!(12.5)));
        assert_eq!(
            changes,
            Changes {
                state: false,
                position: true
            }
        );
        assert_eq!(state.position, 12.5);

        let changes = state.apply(&property("volume", json!(55.0)));
        assert_eq!(
            changes,
            Changes {
                state: true,
                position: false
            }
        );
        assert_eq!(state.volume, 55.0);
    }

    #[test]
    fn unchanged_values_report_nothing() {
        let mut state = loaded(false);
        state.apply(&property("speed", json!(1.25)));
        assert_eq!(
            state.apply(&property("speed", json!(1.25))),
            Changes::default()
        );
    }

    #[test]
    fn unavailable_properties_reset_position_and_duration() {
        let mut state = loaded(false);
        state.apply(&property("time-pos", json!(3.0)));
        state.apply(&property("duration", json!(180.0)));
        state.apply(&Event::PropertyChange {
            id: 2,
            name: "duration".into(),
            data: None,
        });
        state.apply(&Event::PropertyChange {
            id: 1,
            name: "time-pos".into(),
            data: None,
        });
        assert_eq!(state.duration, None);
        assert_eq!(state.position, 0.0);
    }

    #[test]
    fn end_of_file_goes_idle_and_keeps_the_track() {
        let mut state = loaded(false);
        state.track = Some(Track {
            id: 1,
            path: "/a.flac".into(),
            library_id: None,
            title: "A".into(),
            metadata: TrackMetadata::default(),
            artwork: None,
        });
        state.apply(&Event::EndFile {
            reason: EndReason::Eof,
            error: None,
            playlist_entry_id: Some(1),
        });
        assert_eq!(state.status, Status::Idle);
        assert!(state.track.is_some());
        assert_eq!(state.error, None);
    }

    #[test]
    fn playback_errors_are_reported() {
        let mut state = loaded(false);
        state.apply(&Event::EndFile {
            reason: EndReason::Error,
            error: Some("unrecognized file format".into()),
            playlist_entry_id: Some(1),
        });
        assert_eq!(state.status, Status::Idle);
        assert_eq!(
            state.error,
            Some(PlayerError::Playback {
                message: "unrecognized file format".into()
            })
        );
    }

    #[test]
    fn replacing_a_file_is_not_an_end() {
        let mut state = loaded(false);
        state.apply(&Event::EndFile {
            reason: EndReason::Stop,
            error: None,
            playlist_entry_id: Some(1),
        });
        assert_eq!(state.status, Status::Playing);
    }

    #[test]
    fn serializes_for_the_frontend() {
        let json = serde_json::to_value(PlayerState::default()).unwrap();
        assert_eq!(
            json,
            json!({
                "status": "idle", "track": null, "position": 0.0, "duration": null,
                "volume": 80.0, "muted": false, "speed": 1.0, "error": null
            })
        );
        let error = serde_json::to_value(PlayerError::MpvFailed {
            message: "boom".into(),
        })
        .unwrap();
        assert_eq!(error, json!({ "kind": "mpvFailed", "message": "boom" }));
    }
}
