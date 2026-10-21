use serde_json::Value;

/// Why mpv stopped playing a file (`end-file` event).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// Reached the end of the file.
    Eof,
    /// Stopped by a command (`stop`, or `loadfile` replacing it).
    Stop,
    Quit,
    /// Could not be played; see the event's `error`.
    Error,
    /// A playlist or redirect was expanded into new entries.
    Redirect,
    Unknown,
}

impl EndReason {
    fn parse(reason: &str) -> Self {
        match reason {
            "eof" => Self::Eof,
            "stop" => Self::Stop,
            "quit" => Self::Quit,
            "error" => Self::Error,
            "redirect" => Self::Redirect,
            _ => Self::Unknown,
        }
    }
}

/// An event pushed by mpv. Only the ones Vitra reacts to are modelled; the rest are kept raw.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// An observed property changed. `data` is `None` while the property is unavailable
    /// (e.g. `duration` with nothing loaded).
    PropertyChange {
        id: u64,
        name: String,
        data: Option<Value>,
    },
    StartFile {
        playlist_entry_id: Option<i64>,
    },
    FileLoaded,
    EndFile {
        reason: EndReason,
        error: Option<String>,
        playlist_entry_id: Option<i64>,
    },
    Seek,
    PlaybackRestart,
    Shutdown,
    Other {
        name: String,
        raw: Value,
    },
}

impl Event {
    /// Parses an event message (`{"event": "...", ...}`); `None` if it has no event name.
    pub fn from_value(raw: Value) -> Option<Self> {
        let name = raw.get("event")?.as_str()?.to_owned();
        let int = |key: &str| raw.get(key).and_then(Value::as_i64);
        let event = match name.as_str() {
            "property-change" => Event::PropertyChange {
                id: raw.get("id").and_then(Value::as_u64).unwrap_or(0),
                name: raw
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                data: raw.get("data").cloned(),
            },
            "start-file" => Event::StartFile {
                playlist_entry_id: int("playlist_entry_id"),
            },
            "file-loaded" => Event::FileLoaded,
            "end-file" => Event::EndFile {
                reason: EndReason::parse(
                    raw.get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                ),
                error: raw
                    .get("file_error")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                playlist_entry_id: int("playlist_entry_id"),
            },
            "seek" => Event::Seek,
            "playback-restart" => Event::PlaybackRestart,
            "shutdown" => Event::Shutdown,
            _ => Event::Other { name, raw },
        };
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_property_changes() {
        let event = Event::from_value(json!({
            "event": "property-change", "id": 3, "name": "time-pos", "data": 12.5
        }));
        assert_eq!(
            event,
            Some(Event::PropertyChange {
                id: 3,
                name: "time-pos".into(),
                data: Some(json!(12.5)),
            })
        );
    }

    #[test]
    fn unavailable_property_has_no_data() {
        let event = Event::from_value(json!({
            "event": "property-change", "id": 2, "name": "duration"
        }));
        assert!(matches!(
            event,
            Some(Event::PropertyChange { data: None, .. })
        ));
    }

    #[test]
    fn parses_end_file_with_error() {
        let event = Event::from_value(json!({
            "event": "end-file", "reason": "error", "playlist_entry_id": 4,
            "file_error": "unrecognized file format"
        }));
        assert_eq!(
            event,
            Some(Event::EndFile {
                reason: EndReason::Error,
                error: Some("unrecognized file format".into()),
                playlist_entry_id: Some(4),
            })
        );
    }

    #[test]
    fn keeps_unknown_events_raw() {
        let raw = json!({ "event": "audio-reconfig" });
        assert_eq!(
            Event::from_value(raw.clone()),
            Some(Event::Other {
                name: "audio-reconfig".into(),
                raw
            })
        );
    }

    #[test]
    fn ignores_messages_without_event_name() {
        assert_eq!(Event::from_value(json!({ "request_id": 1 })), None);
    }
}
