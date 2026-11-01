//! What each deck does to the sound: the equalizer, loudness normalization, and the
//! meters behind the backdrop's pulse and the level meter.

use std::path::Path;
use std::sync::atomic::Ordering;

use mpv_ipc::Client;
use serde_json::{Value, json};
use tauri::Manager;

use crate::settings::{AudioMeters, SettingsStore};
use crate::sound::{self, Equalizer, LoudnessMeter, Normalization, SoundSettings, loudness};

use super::engine::{command, read_meters};
use super::state::Status;
use super::{Player, level, lock, spectrum};

impl Player {
    /// What the settings ask mpv's audio chain to measure: the loudness while the backdrop's
    /// pulse is on, the spectrum while the level meter is in the window's layout. Changing
    /// the chain can click, so it isn't touched for a window being hidden; reading stops
    /// then instead ([`Self::set_meters`]).
    pub(super) fn audio_meters(&self) -> AudioMeters {
        self.0.app.try_state::<SettingsStore>().map_or(
            AudioMeters {
                pulse: false,
                spectrum: false,
            },
            |settings| settings.get().audio_meters(),
        )
    }

    /// Puts the measuring filters the settings ask for into mpv's audio chain; `replace` takes
    /// out the ones already there first (a fresh mpv has none).
    pub(super) async fn set_meter_filters(&self, client: &Client, replace: bool) {
        if replace {
            for label in [level::FILTER_LABEL, spectrum::FILTER_LABEL] {
                let label = format!("@{label}");
                let _ = command(client, vec![json!("af"), json!("remove"), json!(label)]).await;
            }
        }
        let meters = self.audio_meters();
        let filters = [
            (meters.pulse, level::filter()),
            (meters.spectrum, spectrum::filter()),
        ];
        for (wanted, filter) in filters {
            if wanted
                && let Err(err) =
                    command(client, vec![json!("af"), json!("add"), json!(filter)]).await
            {
                log::warn!("cannot measure the audio: {err}");
            }
        }
    }

    /// Puts the equalizer into a fresh mpv's audio chain, first, if it changes the sound.
    pub(super) async fn add_equalizer(&self, deck: usize, client: &Client) {
        let Some(settings) = self.0.app.try_state::<SettingsStore>() else {
            return;
        };
        // A fresh mpv has no equalizer: whatever the last one had is gone.
        lock(&self.0.equalizer)[deck] = Equalizer::default();
        self.apply_equalizer(deck, client, settings.get().sound.equalizer, false)
            .await;
    }

    /// Makes a deck's equalizer `to`. While a band is dragged (`live`), the change goes to
    /// that band's filter (`af-command`), so the music plays on undisturbed. mpv builds the
    /// chain afresh for every file and seek from the filter as it was added, so the curve to
    /// keep replaces the filter (in place, by its label); switching the equalizer on or off
    /// adds or removes it.
    pub(super) async fn apply_equalizer(
        &self,
        deck: usize,
        client: &Client,
        to: Equalizer,
        live: bool,
    ) {
        let from = std::mem::replace(&mut lock(&self.0.equalizer)[deck], to.clone());
        let result = match (from.active(), to.active()) {
            (true, true) if !live => {
                command(client, vec![json!("af"), json!("add"), json!(to.filter())]).await
            }
            (true, true) => {
                let mut result = Ok(());
                for [name, value, target] in to.commands(&from) {
                    let args = vec![
                        json!("af-command"),
                        json!(sound::EQ_LABEL),
                        json!(name),
                        json!(value),
                        json!(target),
                    ];
                    result = result.and(command(client, args).await);
                }
                result
            }
            (false, true) => {
                command(client, vec![json!("af"), json!("pre"), json!(to.filter())]).await
            }
            (true, false) => {
                let label = format!("@{}", sound::EQ_LABEL);
                command(client, vec![json!("af"), json!("remove"), json!(label)]).await
            }
            (false, false) => Ok(()),
        };
        if let Err(err) = result {
            log::warn!("cannot change the equalizer: {err}");
        }
    }

    pub(super) fn normalization(&self) -> Normalization {
        self.0
            .app
            .try_state::<SettingsStore>()
            .map_or(Normalization::Off, |settings| {
                settings.get().sound.normalization
            })
    }

    /// How mpv evens out loudness: ReplayGain tags (track or album gains), moved to Vitra's
    /// target by the preamp, with clipping prevented. Applies from the next file.
    pub(super) async fn apply_normalization(&self, client: &Client) {
        let mode = match self.normalization() {
            Normalization::Off => "no",
            Normalization::Track => "track",
            Normalization::Album => "album",
        };
        let properties = [
            ("replaygain", json!(mode)),
            ("replaygain-preamp", json!(loudness::REPLAYGAIN_PREAMP)),
            ("replaygain-clip", json!(true)),
        ];
        for (name, value) in properties {
            if let Err(err) = command(client, vec![json!("set_property"), json!(name), value]).await
            {
                log::warn!("cannot set {name}: {err}");
            }
        }
        // Links are measured while they play (see `measure_link`).
        let label = format!("@{}", sound::LOUDNESS_LABEL);
        let _ = command(client, vec![json!("af"), json!("remove"), json!(label)]).await;
        if mode != "no"
            && let Err(err) = command(
                client,
                vec![json!("af"), json!("add"), json!(sound::loudness_filter())],
            )
            .await
        {
            log::warn!("cannot measure loudness: {err}");
        }
    }

    /// Measures a link's loudness while it plays (it can't be measured ahead without
    /// downloading it), from the R128 filter's momentary readings; stored when the track
    /// ends, if enough of it was heard, for the next time it plays.
    pub(super) fn measure_link(&self, id: u64, path: &str) {
        if !links::is_remote(path) || self.normalization() == Normalization::Off {
            return;
        }
        let Some(library) = self.library() else {
            return;
        };
        let player = self.clone();
        let key = path.to_owned();
        tauri::async_runtime::spawn(async move {
            let known = {
                let key = key.clone();
                library.run(move |db| db.loudness(&key, None)).await
            };
            if matches!(known, Ok(Some(_))) {
                return;
            }
            let property = format!("af-metadata/{}", sound::LOUDNESS_LABEL);
            let mut integrator = loudness::Integrator::default();
            let mut duration = None;
            loop {
                tokio::time::sleep(loudness::READING_INTERVAL).await;
                let (current, status) = {
                    let state = lock(&player.0.state);
                    duration = state.duration.or(duration);
                    (state.track.as_ref().map(|track| track.id), state.status)
                };
                if current != Some(id) || status == Status::Idle {
                    break;
                }
                if status != Status::Playing {
                    continue;
                }
                let Ok(client) = player.client() else {
                    continue;
                };
                let reading = client.get_property::<Value>(&property).await.ok();
                let momentary = reading
                    .as_ref()
                    .and_then(|data| data.get("lavfi.r128.M"))
                    .and_then(Value::as_str)
                    .and_then(|text| text.parse::<f64>().ok());
                if let Some(momentary) = momentary {
                    integrator.push(momentary);
                }
            }
            let Some(integrated) = integrator.integrated(duration) else {
                return;
            };
            let loudness = library::Loudness {
                integrated,
                peak: None,
            };
            let stored = {
                let key = key.clone();
                library
                    .run(move |db| db.set_loudness(&key, None, loudness))
                    .await
            };
            match stored {
                Ok(()) => log::info!("loudness of {key}: {integrated:.1} LUFS (while playing)"),
                Err(err) => log::warn!("cannot store the loudness of {key}: {err}"),
            }
        });
    }

    /// Options for one file: with normalization on, the gain from Vitra's measurement for a
    /// file without ReplayGain tags (mpv uses `replaygain-fallback` only then; 0 when it
    /// wasn't measured yet, which also asks for a measurement).
    pub(super) async fn file_options(&self, path: &str) -> serde_json::Map<String, Value> {
        let mut options = serde_json::Map::new();
        let mode = self.normalization();
        if mode == Normalization::Off {
            return options;
        }
        let local = !links::is_remote(path);
        let stamp = if local {
            sound::measure::file_stamp(Path::new(path))
        } else {
            None
        };
        let measured = match self.library() {
            Some(library) if !local || stamp.is_some() => {
                let key = path.to_owned();
                library
                    .run(move |db| {
                        let own = db.loudness(&key, stamp)?;
                        let album = match mode {
                            Normalization::Album if own.is_some() => db.album_loudness(&key)?,
                            _ => None,
                        };
                        Ok(album.or(own))
                    })
                    .await
                    .unwrap_or_else(|err| {
                        log::warn!("cannot read the loudness of {path}: {err}");
                        None
                    })
            }
            _ => None,
        };
        if measured.is_none()
            && local
            && let Some(meter) = self.0.app.try_state::<LoudnessMeter>()
        {
            meter.want(Path::new(path));
        }
        let gain = measured.map_or(0.0, loudness::gain);
        options.insert("replaygain-fallback".into(), json!(format!("{gain:.1}")));
        options
    }

    /// A file was measured: if it is the one waiting to play next, it is handed to mpv again
    /// with its gain.
    pub fn loudness_measured(&self, path: &str) {
        let next = lock(&self.0.mirror).next.map(|(_, uid)| uid);
        let waiting = lock(&self.0.queue)
            .peek_next()
            .filter(|item| Some(item.uid) == next && item.entry.path == path)
            .is_some();
        if !waiting {
            return;
        }
        let Ok(client) = self.client() else {
            return;
        };
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            let _transport = player.0.transport.lock().await;
            if let Err(err) = player.sync_next(&client, true).await {
                log::warn!("cannot queue the next file again: {err}");
            }
        });
    }

    /// The sound settings changed.
    pub fn sound_changed(&self, previous: &SoundSettings, next: &SoundSettings) {
        if previous.equalizer != next.equalizer {
            self.set_equalizer(next.equalizer.clone(), false);
        }
        if previous.crossfade != next.crossfade {
            self.crossfade_changed();
        }
        if previous.normalization != next.normalization || previous.crossfade != next.crossfade {
            let normalization = previous.normalization != next.normalization;
            let player = self.clone();
            tauri::async_runtime::spawn(async move {
                if normalization {
                    for (_, client) in player.clients() {
                        player.apply_normalization(&client).await;
                    }
                }
                // The next file goes to mpv again (with its gain, or to the other deck).
                let _transport = player.0.transport.lock().await;
                if let Ok(client) = player.client()
                    && let Err(err) = player.sync_next(&client, true).await
                {
                    log::warn!("cannot queue the next file again: {err}");
                }
            });
        }
    }

    /// Plays with `equalizer` without saving it (a band being dragged); the settings bring
    /// the one to keep.
    pub fn preview_equalizer(&self, equalizer: Equalizer) {
        self.set_equalizer(equalizer, true);
    }

    pub(super) fn set_equalizer(&self, equalizer: Equalizer, live: bool) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            // One change at a time, so quick moves reach mpv in order.
            let _turn = player.0.equalizer_turn.lock().await;
            for (deck, client) in player.clients() {
                player
                    .apply_equalizer(deck, &client, equalizer.clone(), live)
                    .await;
            }
        });
    }

    /// The backdrop pulse was switched, or the level meter shown or hidden.
    pub fn meter_filters_changed(&self) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            for (_, client) in player.clients() {
                player.set_meter_filters(&client, true).await;
            }
        });
    }

    /// What a window shows: the pulse (it is visible, motion is on and so is the setting)
    /// and the spectrum (the level meter is on screen). While one of them is wanted, they are
    /// read and sent as `player://level` and `player://spectrum`.
    pub fn set_meters(&self, pulse: bool, spectrum: bool) {
        self.0.pulse_wanted.store(pulse, Ordering::Relaxed);
        self.0.spectrum_wanted.store(spectrum, Ordering::Relaxed);
        if (pulse || spectrum) && !self.0.meters_running.swap(true, Ordering::Relaxed) {
            tauri::async_runtime::spawn(read_meters(self.clone()));
        }
    }
}
