//! mpv events become player state, and the `player://` and `queue://` events the
//! windows listen to.
//!
//! A link that fails for a reason the user can fix (cookies, an age check, a sign-in)
//! stops the queue here; anything else is skipped.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use media::TrackMetadata;
use mpv_ipc::{Client, EndReason, Event};
use serde::Serialize;
use serde_json::{Value, json};
use tauri::{Emitter, Manager};

use super::engine::command;
use super::metadata::fill_missing;
use super::state::{PlayerError, Status};
use super::{
    COMMAND_TIMEOUT, Mirror, POSITION_EVENT, Player, PlayerState, QUEUE_EVENT, STATE_EVENT,
    TRACK_EVENT, lock,
};
use crate::now_playing::NowPlaying;
use crate::tray::Tray;
use crate::wave::WaveService;

/// Unplayable files in a row that are skipped before playback stops.
const MAX_SKIPS: usize = 5;

/// Lines of the hook's log kept per link: its errors are a few lines of yt-dlp's stderr.
const HOOK_LOG_LINES: usize = 40;

/// The UI interpolates between position updates, so a few per second is enough.
const POSITION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PositionUpdate {
    position: f64,
    duration: Option<f64>,
}

impl Player {
    pub(super) fn emit_queue_if(&self, changed: bool) {
        if changed {
            self.emit_queue();
        }
    }

    /// Internet radio names the song playing in its stream metadata (`icy-title`, usually
    /// "Artist - Title"): show it in place of the station.
    pub(super) fn stream_title_changed(&self, data: Option<&Value>) {
        let Some(tags) = data.and_then(Value::as_object) else {
            return;
        };
        let tag = |key: &str| {
            tags.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .and_then(|(_, v)| v.as_str())
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let Some(song) = tag("icy-title") else {
            return;
        };
        let station = tag("icy-name");
        let track = {
            let mut state = lock(&self.0.state);
            let Some(track) = state
                .track
                .as_mut()
                .filter(|track| links::is_remote(&track.path))
            else {
                return;
            };
            let (artist, title) = match song.split_once(" - ") {
                Some((artist, title)) if !artist.trim().is_empty() && !title.trim().is_empty() => {
                    (Some(artist.trim().to_owned()), title.trim().to_owned())
                }
                _ => (station.clone(), song),
            };
            if track.title == title && track.metadata.artist == artist {
                return;
            }
            track.metadata.title = Some(title.clone());
            track.title = title;
            track.metadata.artist = artist;
            track.metadata.album = station.or(track.metadata.album.take());
            track.clone()
        };
        self.emit_state();
        self.emit(TRACK_EVENT, &track);
    }

    /// Formats lofty can't read (DSF, WMA, ...) still have tags mpv/FFmpeg understands.
    pub(super) async fn fill_from_mpv_metadata(&self, client: &Client) {
        let needs_tags = lock(&self.0.state)
            .track
            .as_ref()
            .is_some_and(|t| t.metadata.title.is_none());
        if !needs_tags {
            return;
        }
        let Ok(Ok(tags)) = tokio::time::timeout(
            COMMAND_TIMEOUT,
            client.get_property::<serde_json::Map<String, Value>>("metadata"),
        )
        .await
        else {
            return;
        };
        let tag = |key: &str| {
            tags.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .and_then(|(_, v)| v.as_str())
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        let from_mpv = TrackMetadata {
            title: tag("title"),
            artist: tag("artist"),
            album: tag("album"),
            album_artist: tag("album_artist"),
            genre: tag("genre"),
            ..TrackMetadata::default()
        };
        {
            let mut state = lock(&self.0.state);
            let Some(track) = state.track.as_mut() else {
                return;
            };
            fill_missing(&mut track.metadata, from_mpv);
            if let Some(title) = &track.metadata.title {
                track.title.clone_from(title);
            }
        }
        self.emit_state();
    }

    pub(super) async fn handle_event(&self, deck: usize, event: Event, client: &Client) {
        if deck != self.active_deck() {
            self.offstage_event(deck, &event).await;
            return;
        }
        // The ramp moves the volume; the user's stays as it was.
        if lock(&self.0.fade).ramping
            && matches!(&event, Event::PropertyChange { name, .. } if name == "volume")
        {
            return;
        }
        let mut apply = true;
        let mut stop = false;
        let mut link_error = None;
        let mut crossfade_due = false;
        match &event {
            Event::FileLoaded => {
                lock(&self.0.mirror).failures = 0;
                let resume = lock(&self.0.pending_seek).take();
                if let Some(position) = resume {
                    let _ = command(
                        client,
                        vec![json!("seek"), json!(position), json!("absolute+exact")],
                    )
                    .await;
                }
                self.fill_from_mpv_metadata(client).await;
            }
            Event::Other { name, raw } if name == "log-message" => {
                if let Some(line) = hook_line(raw) {
                    let mut log = lock(&self.0.hook_log);
                    if log.len() < HOOK_LOG_LINES {
                        log.push(line);
                    }
                }
                return;
            }
            Event::StartFile { .. } if lock(&self.0.mirror).halted => return,
            Event::StartFile {
                playlist_entry_id: Some(entry),
            } => {
                lock(&self.0.hook_log).clear();
                let preloaded = {
                    let mut mirror = lock(&self.0.mirror);
                    // The first entry of an expanded playlist stands in for it.
                    if mirror.redirected {
                        mirror.redirected = false;
                        mirror.current = mirror.current.map(|(_, uid)| (*entry, uid));
                    }
                    mirror.next.map(|(id, _)| id)
                };
                if preloaded == Some(*entry) {
                    self.advanced(client, *entry).await;
                }
            }
            Event::PropertyChange { name, data, .. } if name == "metadata" => {
                self.stream_title_changed(data.as_ref());
            }
            Event::EndFile {
                reason,
                playlist_entry_id,
                ..
            } => {
                let mut mirror = lock(&self.0.mirror);
                let is_current = mirror
                    .current
                    .is_some_and(|(id, _)| Some(id) == *playlist_entry_id);
                if is_current {
                    // Ended before its crossfade began (the next track was slow to load): the
                    // prepared deck takes over as soon as it can.
                    let has_next = mirror.next.is_some() || lock(&self.0.fade).prep.is_some();
                    match reason {
                        EndReason::Redirect => mirror.redirected = true,
                        // mpv goes straight on to the preloaded file: no idle in between.
                        EndReason::Eof if has_next => apply = false,
                        EndReason::Error => {
                            mirror.failures += 1;
                            link_error = link_failure(&lock(&self.0.hook_log));
                            // A link the user has to do something about stops the queue
                            // too: the next links would fail the same way.
                            if link_error.is_some() || mirror.failures >= MAX_SKIPS {
                                stop = has_next;
                            } else if has_next {
                                // Skip unplayable files quietly, like other players do.
                                log::info!("skipping an unplayable file");
                                apply = false;
                            }
                        }
                        _ => {}
                    }
                    crossfade_due = matches!(reason, EndReason::Eof | EndReason::Error)
                        && mirror.next.is_none()
                        && lock(&self.0.fade).prep.is_some();
                    if apply && matches!(reason, EndReason::Eof | EndReason::Error) {
                        mirror.current = None;
                    }
                }
            }
            _ => {}
        }
        if crossfade_due {
            if let Some(prep) = lock(&self.0.fade).prep.as_mut() {
                prep.due = true;
            }
            self.crossfade_tick();
        }
        if stop {
            match &link_error {
                Some(err) => log::warn!("a link failed ({err}); stopping"),
                None => log::warn!("{MAX_SKIPS} files in a row failed to play; stopping"),
            }
            *lock(&self.0.mirror) = Mirror {
                halted: true,
                ..Mirror::default()
            };
            let _ = command(client, vec![json!("stop")]).await;
        }
        if !apply {
            return;
        }
        let changes = {
            let mut state = lock(&self.0.state);
            let mut changes = state.apply(&event);
            // In place of mpv's "unrecognized file format": the windows translate it.
            if let Some(err) = link_error {
                state.error = Some(PlayerError::Playback {
                    message: err.to_string(),
                });
                changes.state = true;
            }
            changes
        };
        if changes.state {
            self.emit_state();
        } else if changes.position {
            self.emit_position(false);
        }
        if changes.position {
            self.count_play();
            self.follow_lyrics();
            self.crossfade_tick();
        }
        // A length learnt late can make the next transition a crossfade.
        if matches!(&event, Event::PropertyChange { name, .. } if name == "duration")
            && self.crossfade_seconds() > 0.0
            && lock(&self.0.fade).prep.is_none()
        {
            let player = self.clone();
            let client = self.client();
            tauri::async_runtime::spawn(async move {
                let _transport = player.0.transport.lock().await;
                if let Ok(client) = client
                    && let Err(err) = player.sync_next(&client, false).await
                {
                    log::warn!("cannot prepare the next track: {err}");
                }
            });
        }
    }

    pub(super) fn fail(&self, error: PlayerError) {
        {
            let mut state = lock(&self.0.state);
            state.error = Some(error);
            state.status = Status::Idle;
        }
        self.emit_state();
    }

    pub(super) fn emit_state(&self) {
        let state = self.state();
        *lock(&self.0.last_position_emit) = Instant::now();
        self.emit(STATE_EVENT, &state);
        self.show_now_playing(&state);
        if let Some(tray) = self.0.app.try_state::<Tray>() {
            tray.update(&state);
        }
    }

    /// Tells the system's Now Playing (it sends only what changed).
    pub(super) fn show_now_playing(&self, state: &PlayerState) {
        if let Some(now_playing) = self.0.app.try_state::<NowPlaying>() {
            now_playing.update(state, &self.0.artwork);
        }
    }

    pub(super) fn emit_position(&self, force: bool) {
        {
            let mut last = lock(&self.0.last_position_emit);
            if !force && last.elapsed() < POSITION_INTERVAL {
                return;
            }
            *last = Instant::now();
        }
        let state = self.state();
        let update = PositionUpdate {
            position: state.position,
            duration: state.duration,
        };
        self.emit(POSITION_EVENT, &update);
        // A seek shows up here as a jump in the position.
        self.show_now_playing(&state);
    }

    pub(super) fn emit_queue(&self) {
        self.0.queue_dirty.store(true, Ordering::Relaxed);
        let view = self.queue();
        self.emit(QUEUE_EVENT, &view);
        // My Wave keeps a few tracks ahead of the playing one.
        let wants_more = {
            let queue = lock(&self.0.queue);
            queue.is_wave() && queue.upcoming() < crate::wave::AHEAD
        };
        if wants_more && let Some(wave) = self.0.app.try_state::<WaveService>() {
            wave.top_up();
        }
    }

    pub(super) fn emit<S: Serialize + Clone>(&self, event: &str, payload: &S) {
        if let Err(err) = self.0.app.emit(event, payload) {
            log::warn!("cannot emit {event}: {err}");
        }
    }
}

/// The text of a `log-message` event from mpv's yt-dlp hook (warnings and errors).
fn hook_line(raw: &Value) -> Option<String> {
    let field = |key: &str| raw.get(key).and_then(Value::as_str);
    let level = field("level")?;
    (field("prefix")? == "ytdl_hook" && matches!(level, "error" | "warn" | "fatal"))
        .then(|| field("text").map(|text| text.trim_end().to_owned()))
        .flatten()
}

/// Why a link failed to open, from its hook's log, when the user can do something about it
/// (cookies, an age limit, a sign-in): then playback stops with that instead of skipping on,
/// since the next links would fail the same way.
fn link_failure(log: &[String]) -> Option<links::LinkError> {
    if log.is_empty() {
        return None;
    }
    match links::ytdlp::failure(&log.join("\n")) {
        links::LinkError::Failed(_) => None,
        known => Some(known),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_yt_dlp_hook_explains_failed_links() {
        let message = |prefix: &str, level: &str, text: &str| json!({"event": "log-message", "prefix": prefix, "level": level, "text": text});
        let cookies = "ERROR: [Errno 1] Operation not permitted: \
            '/Users/me/Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies'\n";
        let line = hook_line(&message("ytdl_hook", "error", cookies)).unwrap();
        assert!(line.ends_with("Cookies.binarycookies'"), "trimmed: {line}");
        // Other modules and chatter are left out.
        assert_eq!(hook_line(&message("cplayer", "error", "boom")), None);
        assert_eq!(hook_line(&message("ytdl_hook", "v", "status: 1")), None);

        assert!(matches!(
            link_failure(&[line]),
            Some(links::LinkError::Cookies(_))
        ));
        let age = "ERROR: [youtube] abc: Sign in to confirm your age. This video may be \
            inappropriate for some users."
            .to_owned();
        assert!(matches!(
            link_failure(&[age]),
            Some(links::LinkError::AgeRestricted)
        ));
        // Errors without a remedy keep the queue going.
        assert!(
            link_failure(&["ERROR: Unsupported URL: https://example.com".to_owned()]).is_none()
        );
        assert!(link_failure(&[]).is_none());
    }
}
