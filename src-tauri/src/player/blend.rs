//! Crossfade on two mpv decks.
//!
//! The curves, the volume law and the "same album" rule live in [`super::crossfade`].
//! This module loads the next track on the standby deck and ramps the two volumes.

use std::time::{Duration, Instant};

use mpv_ipc::{EndReason, Event};
use serde_json::json;
use tauri::Manager;

use crate::settings::SettingsStore;

use super::crossfade;
use super::engine::{command, entry_id, loadfile};
use super::queue::QueueItem;
use super::state::Status;
use super::{Mirror, Player, Prep, lock};

impl Player {
    pub(super) fn crossfade_seconds(&self) -> f64 {
        self.0
            .app
            .try_state::<SettingsStore>()
            .map_or(0.0, |settings| settings.get().sound.crossfade)
    }

    /// Whether `current` blends into `next`: crossfade is on, the other deck is up, the
    /// tracks aren't neighbours on one album (or the same item again), and `current` is
    /// long enough (a radio stream, of unknown length, isn't blended).
    pub(super) fn crossfades(&self, current: &QueueItem, next: &QueueItem) -> bool {
        let seconds = self.crossfade_seconds();
        let duration = lock(&self.0.state).duration.or(current.entry.duration);
        seconds > 0.0
            && next.uid != current.uid
            && lock(&self.0.fade).failed != Some(next.uid)
            && !crossfade::same_album(&current.entry, &next.entry)
            && duration.is_some_and(|d| crossfade::start_at(d, seconds).is_some())
            && self.deck_client(1 - self.active_deck()).is_some()
    }

    /// Loads `item` on the other deck, paused and silent, to start when the playing track
    /// is about to end. A deck still fading the last track out is busy: the ramp's end comes
    /// back here.
    pub(super) async fn prepare_crossfade(&self, item: &QueueItem) {
        let standby = 1 - self.active_deck();
        {
            let mut fade = lock(&self.0.fade);
            if fade.outgoing == Some(standby)
                || fade
                    .prep
                    .is_some_and(|prep| prep.uid == item.uid && prep.deck == standby)
            {
                return;
            }
            fade.prep = Some(Prep {
                deck: standby,
                uid: item.uid,
                entry: None,
                ready: false,
                armed: false,
                due: false,
            });
        }
        let Some(client) = self.deck_client(standby) else {
            lock(&self.0.fade).prep = None;
            return;
        };
        let state = self.state();
        let quiet = [
            ("pause", json!(true)),
            ("volume", json!(0)),
            ("mute", json!(state.muted)),
            ("speed", json!(state.speed)),
        ];
        for (name, value) in quiet {
            let _ = command(&client, vec![json!("set_property"), json!(name), value]).await;
        }
        let options = self.file_options(&item.entry.path).await;
        match loadfile(&client, &item.entry.path, "replace", options).await {
            Ok(reply) => {
                let mut fade = lock(&self.0.fade);
                if let Some(prep) = fade.prep.as_mut().filter(|prep| prep.uid == item.uid) {
                    prep.entry = entry_id(&reply);
                }
            }
            Err(err) => {
                log::warn!("cannot load the next track on the other deck: {err}");
                let mut fade = lock(&self.0.fade);
                fade.prep = None;
                fade.failed = Some(item.uid);
            }
        }
    }

    /// Forgets a prepared crossfade (the next track changed, or follows gapless now).
    pub(super) fn drop_prep(&self) {
        let prep = lock(&self.0.fade).prep.take();
        if let Some(client) = prep.and_then(|prep| self.deck_client(prep.deck)) {
            tauri::async_runtime::spawn(async move {
                let _ = command(&client, vec![json!("stop")]).await;
            });
        }
    }

    /// Ends a crossfade at once: the track fading out stops, the new one plays at full
    /// volume (a seek).
    pub(super) async fn end_fade(&self) {
        let outgoing = {
            let mut fade = lock(&self.0.fade);
            if !fade.ramping && fade.outgoing.is_none() {
                return;
            }
            fade.ramping = false;
            fade.outgoing.take()
        };
        let volume = lock(&self.0.state).volume;
        if let Some(client) = outgoing.and_then(|deck| self.deck_client(deck)) {
            let _ = command(&client, vec![json!("stop")]).await;
            let _ = command(
                &client,
                vec![json!("set_property"), json!("volume"), json!(volume)],
            )
            .await;
        }
        if let Ok(client) = self.client() {
            let _ = command(
                &client,
                vec![json!("set_property"), json!("volume"), json!(volume)],
            )
            .await;
        }
    }

    /// Calls a crossfade off: something else plays now (the user chose another track).
    pub(super) fn cancel_crossfade(&self) {
        self.drop_prep();
        let player = self.clone();
        tauri::async_runtime::spawn(async move { player.end_fade().await });
    }

    /// The playing track moved on: schedules the prepared crossfade's start once it is close
    /// (or at once, if the track already ended or passed the point).
    pub(super) fn crossfade_tick(&self) {
        let (position, duration, speed, status) = {
            let state = lock(&self.0.state);
            (state.position, state.duration, state.speed, state.status)
        };
        let seconds = self.crossfade_seconds();
        let due = lock(&self.0.fade).prep.is_some_and(|prep| prep.due);
        let wait = match (
            status,
            duration.and_then(|d| crossfade::start_at(d, seconds)),
        ) {
            _ if due => 0.0,
            (Status::Idle, _) => 0.0,
            (Status::Playing, Some(at)) => (at - position) / speed.max(0.25),
            _ => return,
        };
        if wait > 1.0 {
            return;
        }
        let uid = {
            let mut fade = lock(&self.0.fade);
            let Some(prep) = fade.prep.as_mut().filter(|prep| prep.ready && !prep.armed) else {
                return;
            };
            prep.armed = true;
            prep.uid
        };
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs_f64(wait.max(0.0))).await;
            player.start_crossfade(uid).await;
        });
    }

    /// The other deck starts the next track and becomes the player's; the volumes cross.
    pub(super) async fn start_crossfade(&self, uid: u64) {
        let _transport = self.0.transport.lock().await;
        let prep = {
            let mut fade = lock(&self.0.fade);
            match fade.prep {
                Some(prep) if prep.uid == uid && prep.ready => fade.prep.take(),
                _ => None,
            }
        };
        let Some(prep) = prep else {
            return;
        };
        let next = lock(&self.0.queue).peek_next().map(|item| item.uid);
        let Some(incoming) = self.deck_client(prep.deck).filter(|_| next == Some(uid)) else {
            return;
        };
        let outgoing = self.active_deck();
        let (playing, volume) = {
            let state = lock(&self.0.state);
            (state.status == Status::Playing, state.volume)
        };
        // After the last track ended there is nothing to blend with: full volume at once.
        let fading_out = playing && !prep.due;
        if !fading_out {
            let _ = command(
                &incoming,
                vec![json!("set_property"), json!("volume"), json!(volume)],
            )
            .await;
        }
        if let Err(err) = command(
            &incoming,
            vec![json!("set_property"), json!("pause"), json!(false)],
        )
        .await
        {
            log::warn!("cannot start the next track: {err}");
            return;
        }
        self.0
            .decks
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .active = prep.deck;
        {
            let mut fade = lock(&self.0.fade);
            fade.outgoing = fading_out.then_some(outgoing);
            fade.ramping = fading_out;
        }
        *lock(&self.0.mirror) = Mirror {
            current: prep.entry.map(|entry| (entry, uid)),
            ..Mirror::default()
        };
        let item = lock(&self.0.queue).jump(uid).cloned();
        if let Some(item) = &item {
            self.begin_track(item);
        }
        // The file is loaded already: it plays from here.
        let duration = incoming
            .get_property::<f64>("duration")
            .await
            .ok()
            .filter(|d| d.is_finite() && *d > 0.0);
        {
            let mut state = lock(&self.0.state);
            state.status = Status::Playing;
            state.paused = false;
            state.duration = duration.or(state.duration);
        }
        self.emit_state();
        self.emit_queue();
        self.fill_from_mpv_metadata(&incoming).await;
        let player = self.clone();
        if fading_out {
            tauri::async_runtime::spawn(async move { player.ramp(prep.deck, outgoing).await });
        } else {
            // The old deck is free: prepare what follows.
            if let Some(client) = self.deck_client(outgoing) {
                let _ = command(&client, vec![json!("stop")]).await;
            }
            tauri::async_runtime::spawn(async move {
                let _transport = player.0.transport.lock().await;
                if let Ok(client) = player.client()
                    && let Err(err) = player.sync_next(&client, true).await
                {
                    log::warn!("cannot prepare the next track: {err}");
                }
            });
        }
    }

    /// Crosses the two decks' volumes over the crossfade's length (paused time doesn't
    /// count), then stops the old track and prepares the next crossfade.
    pub(super) async fn ramp(&self, incoming: usize, outgoing: usize) {
        let speed = lock(&self.0.state).speed.max(0.25);
        let length = (self.crossfade_seconds() / speed).max(0.1);
        let mut elapsed = 0.0;
        let mut last = Instant::now();
        loop {
            tokio::time::sleep(crossfade::RAMP_STEP).await;
            let now = Instant::now();
            let (paused, volume) = {
                let state = lock(&self.0.state);
                (state.paused, state.volume)
            };
            if !paused {
                elapsed += now.duration_since(last).as_secs_f64();
            }
            last = now;
            // Called off (a seek, another track).
            if !lock(&self.0.fade).ramping || self.active_deck() != incoming {
                break;
            }
            let progress = elapsed / length;
            let (rise, fall) = crossfade::curves(progress);
            let moves = [
                (incoming, crossfade::deck_volume(volume, rise)),
                (outgoing, crossfade::deck_volume(volume, fall)),
            ];
            for (deck, value) in moves {
                if let Some(client) = self.deck_client(deck) {
                    let _ = command(
                        &client,
                        vec![json!("set_property"), json!("volume"), json!(value)],
                    )
                    .await;
                }
            }
            if progress >= 1.0 {
                break;
            }
        }
        self.end_fade().await;
        // The other deck is free again: the next crossfade can be prepared on it.
        let _transport = self.0.transport.lock().await;
        if let Ok(client) = self.client()
            && let Err(err) = self.sync_next(&client, true).await
        {
            log::warn!("cannot prepare the next track: {err}");
        }
    }

    /// An event of the deck that isn't playing the current track: the next track loaded (or
    /// failed to) on it, or the track fading out ended.
    pub(super) async fn offstage_event(&self, deck: usize, event: &Event) {
        match event {
            Event::FileLoaded => {
                let ready = {
                    let mut fade = lock(&self.0.fade);
                    match fade.prep.as_mut().filter(|prep| prep.deck == deck) {
                        Some(prep) => {
                            prep.ready = true;
                            true
                        }
                        None => false,
                    }
                };
                if ready {
                    self.crossfade_tick();
                }
            }
            Event::EndFile {
                reason: EndReason::Error,
                ..
            } => {
                let failed = {
                    let mut fade = lock(&self.0.fade);
                    let prep = fade.prep.filter(|prep| prep.deck == deck);
                    if let Some(prep) = prep {
                        fade.prep = None;
                        fade.failed = Some(prep.uid);
                    }
                    prep.is_some()
                };
                // The next track follows gapless on this deck instead (or is skipped there
                // like any file that can't play).
                if failed && let Ok(client) = self.client() {
                    let player = self.clone();
                    tauri::async_runtime::spawn(async move {
                        let _transport = player.0.transport.lock().await;
                        if let Err(err) = player.sync_next(&client, true).await {
                            log::warn!("cannot queue the next track: {err}");
                        }
                    });
                }
            }
            _ => {}
        }
    }
}
