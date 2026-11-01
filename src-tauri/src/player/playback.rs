//! The play queue and the transport controls.
//!
//! Opening, enqueueing and skipping all go through one lock, so two quick actions
//! can't interleave their `loadfile` commands. What mpv is actually told lives in
//! [`super::engine`].

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

use media::TrackMetadata;
use mpv_ipc::Client;
use serde_json::json;
use tauri::Manager;

use crate::error::AppError;
use crate::library::{LibraryService, entry_from_track};
use crate::settings::SettingsStore;
use crate::wave::WaveService;

use super::engine::{command, entry_id, loadfile};
use super::queue::{Entry, Queue, QueueItem, Repeat};
use super::state::{Status, Track};
use super::{Mirror, Player, lock};

/// Previous restarts the track instead of going back once it has played this long.
const RESTART_THRESHOLD: f64 = 3.0;

/// A play counts once half the track (at most four minutes) has been heard.
const COUNT_AFTER_MAX: f64 = 240.0;

/// Opening a folder queues at most this many files.
const MAX_OPEN: usize = 10_000;

impl Player {
    /// Plays files and folders (opened, dropped or passed at launch), replacing the queue.
    pub async fn open(&self, paths: Vec<PathBuf>) -> Result<(), AppError> {
        let files = tauri::async_runtime::spawn_blocking(move || expand(paths)).await?;
        if files.is_empty() {
            return Err(AppError::NotFound("no playable files".into()));
        }
        let entries = self.file_entries(files.clone()).await;
        let untagged: Vec<String> = entries
            .iter()
            .filter(|entry| entry.track_id.is_none())
            .map(|entry| entry.path.clone())
            .collect();
        self.play_entries(entries, 0).await?;
        let library = self.library().filter(|_| self.adds_opened_files());
        match library {
            // Playback starts right away; the files join the library in the background.
            Some(library) if !untagged.is_empty() => self.import(library, files),
            _ => self.read_entry_tags(untagged),
        }
        Ok(())
    }

    pub(super) fn adds_opened_files(&self) -> bool {
        self.0
            .app
            .try_state::<SettingsStore>()
            .is_some_and(|settings| settings.get().library.add_opened_files)
    }

    /// Adds opened files to the library, then links their queue entries to the new tracks.
    pub(super) fn import(&self, library: LibraryService, files: Vec<PathBuf>) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            match tauri::async_runtime::spawn_blocking(move || library.add_files(&files)).await {
                Ok(Ok(tracks)) => player.link_tracks(&tracks),
                Ok(Err(err)) => log::warn!("cannot add opened files to the library: {err}"),
                Err(err) => log::warn!("adding opened files failed: {err}"),
            }
        });
    }

    /// Queued files became library tracks: give their entries (and the playing track) the
    /// track ids, so favorites, history and playlists work for them.
    pub(super) fn link_tracks(&self, tracks: &[library::Track]) {
        let mut queued = false;
        {
            let mut queue = lock(&self.0.queue);
            for track in tracks {
                let entry = entry_from_track(track.clone());
                queued |= queue.update_entries(&track.path, |queued| *queued = entry.clone());
            }
        }
        let playing = {
            let mut state = lock(&self.0.state);
            match state.track.as_mut() {
                Some(current) => tracks
                    .iter()
                    .find(|track| track.path == current.path)
                    .is_some_and(|track| {
                        let linked = current.library_id != Some(track.id);
                        current.library_id = Some(track.id);
                        linked
                    }),
                None => false,
            }
        };
        if queued {
            self.emit_queue();
        }
        if playing {
            self.emit_state();
            self.library_id_changed();
        }
    }

    /// Tracks left the library: their queue entries become plain files again.
    pub fn tracks_removed(&self, ids: &[i64]) {
        let queued = lock(&self.0.queue).update_where(
            |entry| entry.track_id.is_some_and(|id| ids.contains(&id)),
            |entry| {
                entry.track_id = None;
                entry.cover = None;
            },
        );
        let playing = {
            let mut state = lock(&self.0.state);
            match state.track.as_mut() {
                Some(track) if track.library_id.is_some_and(|id| ids.contains(&id)) => {
                    track.library_id = None;
                    true
                }
                _ => false,
            }
        };
        if queued {
            self.emit_queue();
        }
        if playing {
            self.emit_state();
            self.library_id_changed();
        }
    }

    /// Library tracks got more complete metadata (a playlist of links being filled in) or a
    /// cover (a link album's, one the user chose): refresh their queue entries and, if one
    /// is playing, the current track.
    pub fn library_tracks_updated(&self, tracks: &[library::Track]) {
        let queued = {
            let mut queue = lock(&self.0.queue);
            let mut changed = false;
            for track in tracks {
                let entry = entry_from_track(track.clone());
                changed |= queue.update_entries(&track.path, |queued| *queued = entry.clone());
            }
            changed
        };
        let current = {
            let mut state = lock(&self.0.state);
            let duration_known = state.duration.is_some();
            state.track.as_mut().and_then(|current| {
                let track = tracks.iter().find(|track| track.path == current.path)?;
                current.library_id = Some(track.id);
                current.title.clone_from(&track.title);
                current.metadata.title = Some(track.title.clone());
                current.metadata.artist.clone_from(&track.artist);
                current.metadata.album.clone_from(&track.album);
                if !duration_known {
                    current.metadata.duration = track.duration;
                }
                Some((current.id, current.path.clone()))
            })
        };
        self.emit_queue_if(queued);
        if let Some((id, path)) = current {
            self.emit_state();
            self.read_metadata(id, PathBuf::from(path));
        }
    }

    /// A link was downloaded: its queue entries play the file from now on. The playing
    /// stream (if it is one of them) goes on undisturbed.
    pub fn link_downloaded(&self, url: &str, track: &library::Track) {
        let entry = entry_from_track(track.clone());
        let queued = lock(&self.0.queue).update_entries(url, |queued| *queued = entry.clone());
        self.emit_queue_if(queued);
    }

    /// Queue entries for files: library rows where the library knows them, else just a title
    /// from the file name until [`Self::read_entry_tags`] fills in more.
    pub(super) async fn file_entries(&self, files: Vec<PathBuf>) -> Vec<Entry> {
        let paths: Vec<String> = files
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect();
        let known = match self.library() {
            Some(library) => {
                let paths = paths.clone();
                library
                    .run(move |db| {
                        paths
                            .iter()
                            .map(|path| db.track_by_path(path))
                            .collect::<library::Result<Vec<_>>>()
                    })
                    .await
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| match known.get(index).cloned().flatten() {
                Some(track) => entry_from_track(track),
                None => Entry {
                    title: file_title(Path::new(&path)),
                    path,
                    ..Entry::default()
                },
            })
            .collect()
    }

    /// Reads tags of queued files outside the library in the background, so the queue shows
    /// artists and durations.
    pub(super) fn read_entry_tags(&self, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            for chunk in paths.chunks(50) {
                let chunk = chunk.to_vec();
                let Ok(read) = tauri::async_runtime::spawn_blocking(move || {
                    chunk
                        .into_iter()
                        .filter_map(|path| {
                            let info = media::read_track(Path::new(&path)).ok()?;
                            Some((path, info.metadata))
                        })
                        .collect::<Vec<_>>()
                })
                .await
                else {
                    return;
                };
                let mut changed = false;
                {
                    let mut queue = lock(&player.0.queue);
                    for (path, metadata) in read {
                        changed |= queue.update_entries(&path, |entry| {
                            fill_entry(entry, &metadata);
                        });
                    }
                }
                if changed {
                    player.queue_changed_quietly();
                }
            }
        });
    }

    /// Replaces the queue and plays from `start`.
    pub async fn play_entries(&self, entries: Vec<Entry>, start: usize) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let item = {
            let mut queue = lock(&self.0.queue);
            queue.replace(entries, start, seed());
            queue.current().cloned()
        };
        self.emit_queue();
        match item {
            Some(item) => self.load(&item).await,
            None => Ok(()),
        }
    }

    /// Starts My Wave with its first picks. With `from_current` (a wave like the playing
    /// track), that track goes on and the picks follow it; otherwise they replace the queue.
    pub async fn play_wave(
        &self,
        entries: Vec<Entry>,
        mode: mood::Mode,
        from_current: bool,
    ) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let load = {
            let mut queue = lock(&self.0.queue);
            if from_current && queue.current().is_some() {
                queue.wave_from_current(entries, mode);
                None
            } else {
                queue.replace_wave(entries, mode);
                queue.current().cloned()
            }
        };
        match load {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => self.queue_changed().await,
        }
    }

    /// Adds the wave's next picks at the end, if the queue is still a wave. Playback that
    /// stopped at the wave's end (it ran out meanwhile) goes on with them.
    pub async fn append_wave(&self, entries: Vec<Entry>) -> Result<bool, AppError> {
        if entries.is_empty() {
            return Ok(false);
        }
        let _transport = self.0.transport.lock().await;
        let resume = {
            let mut queue = lock(&self.0.queue);
            if !queue.is_wave() {
                return Ok(false);
            }
            let ended = queue.upcoming() == 0 && lock(&self.0.state).status == Status::Idle;
            queue.append(entries);
            if ended {
                queue.advance(true).cloned()
            } else {
                None
            }
        };
        match resume {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await?;
            }
            None => self.queue_changed().await?,
        }
        Ok(true)
    }

    /// The wave ends; its tracks stay queued.
    pub fn end_wave(&self) {
        let ended = {
            let mut queue = lock(&self.0.queue);
            let was = queue.is_wave();
            queue.end_wave();
            was
        };
        if ended {
            self.emit_queue();
        }
    }

    /// Adds entries after the current item or at the end. Adding to an empty queue starts
    /// playing.
    pub async fn enqueue(&self, entries: Vec<Entry>, next: bool) -> Result<(), AppError> {
        if entries.is_empty() {
            return Ok(());
        }
        let _transport = self.0.transport.lock().await;
        let start = {
            let mut queue = lock(&self.0.queue);
            if next {
                queue.play_next(entries);
            } else {
                queue.append(entries);
            }
            if queue.current().is_none() {
                let first = queue.items().first().map(|item| item.uid);
                first.and_then(|uid| queue.jump(uid).cloned())
            } else {
                None
            }
        };
        match start {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => self.queue_changed().await,
        }
    }

    pub async fn jump(&self, uid: u64) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        let item = lock(&self.0.queue).jump(uid).cloned();
        match item {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => Ok(()),
        }
    }

    pub async fn next(&self) -> Result<(), AppError> {
        self.note_skip();
        let _transport = self.0.transport.lock().await;
        let item = lock(&self.0.queue).advance(true).cloned();
        match item {
            Some(item) => {
                self.emit_queue();
                self.load(&item).await
            }
            None => Ok(()),
        }
    }

    /// Restarts the track, or goes to the previous one when near its start.
    pub async fn previous(&self) -> Result<(), AppError> {
        let (position, status) = {
            let state = lock(&self.0.state);
            (state.position, state.status)
        };
        if position <= RESTART_THRESHOLD || status == Status::Idle {
            let _transport = self.0.transport.lock().await;
            let item = lock(&self.0.queue).back().cloned();
            if let Some(item) = item {
                self.emit_queue();
                return self.load(&item).await;
            }
        }
        if status == Status::Idle {
            return Ok(());
        }
        self.seek(0.0).await
    }

    /// Applies an edit that doesn't change the playing item.
    pub async fn edit_queue(&self, edit: impl FnOnce(&mut Queue)) -> Result<(), AppError> {
        let _transport = self.0.transport.lock().await;
        edit(&mut lock(&self.0.queue));
        self.queue_changed().await
    }

    pub async fn remove(&self, uids: Vec<u64>) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.remove(&uids)).await
    }

    pub async fn move_item(&self, from: usize, to: usize) -> Result<(), AppError> {
        self.edit_queue(|queue| {
            queue.move_item(from, to);
        })
        .await
    }

    pub async fn clear_upcoming(&self) -> Result<(), AppError> {
        self.edit_queue(Queue::clear_upcoming).await
    }

    pub async fn set_shuffle(&self, shuffle: bool) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.set_shuffle(shuffle, seed()))
            .await
    }

    pub async fn set_repeat(&self, repeat: Repeat) -> Result<(), AppError> {
        self.edit_queue(|queue| queue.set_repeat(repeat)).await
    }

    /// Shows a queue item as the current track and reads its tags and artwork.
    pub(super) fn begin_track(&self, item: &QueueItem) {
        let id = self.0.next_track_id.fetch_add(1, Ordering::Relaxed);
        let entry = &item.entry;
        {
            let mut state = lock(&self.0.state);
            // Tracks from the same folder usually share a cover: keep showing it until the
            // new one is read, instead of flashing an empty backdrop between them.
            let same_folder = !links::is_remote(&entry.path)
                && state.track.as_ref().is_some_and(|track| {
                    Path::new(&track.path).parent() == Path::new(&entry.path).parent()
                });
            let artwork = state
                .track
                .take()
                .and_then(|track| track.artwork)
                .filter(|_| same_folder);
            state.track = Some(Track {
                id,
                path: entry.path.clone(),
                library_id: entry.track_id,
                title: entry.title.clone(),
                metadata: TrackMetadata {
                    // A link's entry is all there is to know (the stream's own tags, which
                    // mpv would fill in, are often the uploader's file names).
                    title: links::is_remote(&entry.path).then(|| entry.title.clone()),
                    artist: entry.artist.clone(),
                    album: entry.album.clone(),
                    duration: entry.duration,
                    ..TrackMetadata::default()
                },
                artwork,
            });
            state.status = Status::Loading;
            state.position = 0.0;
            state.duration = entry.duration;
            state.error = None;
        }
        lock(&self.0.mirror).counted = false;
        // A seek asked for while the previous track was loading doesn't apply to this one.
        *lock(&self.0.pending_seek) = None;
        self.emit_state();
        if let Some(lyrics) = self.lyrics() {
            lyrics.track_started(id, &entry.path, entry.track_id);
        }
        self.measure_link(id, &entry.path);
        self.read_metadata(id, PathBuf::from(&entry.path));
    }

    /// Plays a queue item from the start.
    pub(super) async fn load(&self, item: &QueueItem) -> Result<(), AppError> {
        self.cancel_crossfade();
        self.begin_track(item);
        // Files opened at launch arrive before mpv is up.
        self.0.load_waiting.store(true, Ordering::Relaxed);
        let client = self.wait_for_client().await;
        self.0.load_waiting.store(false, Ordering::Relaxed);
        let client = client?;
        command(
            &client,
            vec![json!("set_property"), json!("pause"), json!(false)],
        )
        .await?;
        self.load_with(&client, item).await
    }

    /// Makes `item` mpv's playing file and preloads the one after it.
    pub(super) async fn load_with(
        &self,
        client: &Client,
        item: &QueueItem,
    ) -> Result<(), AppError> {
        *lock(&self.0.mirror) = Mirror::default();
        let options = self.file_options(&item.entry.path).await;
        let reply = loadfile(client, &item.entry.path, "replace", options).await?;
        lock(&self.0.mirror).current = entry_id(&reply).map(|id| (id, item.uid));
        self.sync_next(client, true).await
    }

    /// Mirrors the queue's next item into mpv's playlist (unless it's there already), or,
    /// when the two tracks are to be blended, loads it on the other deck instead.
    pub(super) async fn sync_next(&self, client: &Client, force: bool) -> Result<(), AppError> {
        let (current, wanted) = {
            let queue = lock(&self.0.queue);
            (queue.current().cloned(), queue.peek_next().cloned())
        };
        if lock(&self.0.mirror).current.is_none() {
            // Nothing playing, nothing to preload.
            return Ok(());
        }
        if let (Some(current), Some(next)) = (&current, &wanted)
            && self.crossfades(current, next)
        {
            // mpv mustn't move on by itself: its playlist keeps only the playing file.
            if lock(&self.0.mirror).next.take().is_some() {
                command(client, vec![json!("playlist-clear")]).await?;
            }
            self.prepare_crossfade(next).await;
            return Ok(());
        }
        self.drop_prep();
        {
            let mirror = lock(&self.0.mirror);
            if !force && mirror.next.map(|(_, uid)| uid) == wanted.as_ref().map(|i| i.uid) {
                return Ok(());
            }
        }
        // Clears everything but the playing file.
        command(client, vec![json!("playlist-clear")]).await?;
        lock(&self.0.mirror).next = None;
        if let Some(item) = wanted {
            let options = self.file_options(&item.entry.path).await;
            let reply = loadfile(client, &item.entry.path, "append", options).await?;
            lock(&self.0.mirror).next = entry_id(&reply).map(|id| (id, item.uid));
        }
        Ok(())
    }

    /// mpv moved on to the preloaded file by itself (gapless): follow it in the queue.
    pub(super) async fn advanced(&self, client: &Client, entry: i64) {
        let _transport = self.0.transport.lock().await;
        let uid = {
            let mut mirror = lock(&self.0.mirror);
            // A user action may have replaced the playlist meanwhile.
            let Some((id, uid)) = mirror.next.filter(|(id, _)| *id == entry) else {
                return;
            };
            mirror.current = Some((id, uid));
            mirror.next = None;
            uid
        };
        let item = lock(&self.0.queue).jump(uid).cloned();
        if let Some(item) = &item {
            self.begin_track(item);
        }
        self.emit_queue();
        if let Err(err) = self.sync_next(client, true).await {
            log::warn!("cannot preload the next track: {err}");
        }
    }

    pub(super) async fn queue_changed(&self) -> Result<(), AppError> {
        self.emit_queue();
        match self.client() {
            Ok(client) => self.sync_next(&client, false).await,
            Err(_) => Ok(()),
        }
    }

    /// Display-only queue changes (tags read in the background).
    pub(super) fn queue_changed_quietly(&self) {
        self.emit_queue();
    }

    /// Next before the play counted: a skip, which My Wave weighs (in a wave or not).
    pub(super) fn note_skip(&self) {
        let (track, status) = {
            let state = lock(&self.0.state);
            (
                state.track.as_ref().and_then(|t| t.library_id),
                state.status,
            )
        };
        let Some(track_id) = track else {
            return;
        };
        if lock(&self.0.mirror).counted || status == Status::Idle {
            return;
        }
        let in_wave = lock(&self.0.queue).is_wave();
        if let Some(wave) = self.0.app.try_state::<WaveService>() {
            wave.skipped(track_id, in_wave);
        }
    }

    /// Counts a play of a library track once enough of it was heard.
    pub(super) fn count_play(&self) {
        let (position, duration, library_id) = {
            let state = lock(&self.0.state);
            (
                state.position,
                state.duration,
                state.track.as_ref().and_then(|t| t.library_id),
            )
        };
        let Some(track_id) = library_id else {
            return;
        };
        let threshold = duration
            .filter(|d| *d > 0.0)
            .map_or(30.0, |d| (d / 2.0).min(COUNT_AFTER_MAX));
        if position < threshold {
            return;
        }
        {
            let mut mirror = lock(&self.0.mirror);
            if mirror.counted {
                return;
            }
            mirror.counted = true;
        }
        if let Some(library) = self.library() {
            library.record_play(track_id);
        }
        if lock(&self.0.queue).is_wave()
            && let Some(wave) = self.0.app.try_state::<WaveService>()
        {
            wave.listened(track_id);
        }
    }

    /// A file's tags were edited: refresh its queue entries and, if it is playing, the
    /// current track.
    pub async fn file_retagged(&self, path: PathBuf) {
        let key = path.to_string_lossy().into_owned();
        let entry = match self.library() {
            Some(library) => {
                let key = key.clone();
                library
                    .run(move |db| db.track_by_path(&key))
                    .await
                    .ok()
                    .flatten()
                    .map(entry_from_track)
            }
            None => None,
        };
        let entry = match entry {
            Some(entry) => entry,
            None => {
                let file = path.clone();
                let metadata = tauri::async_runtime::spawn_blocking(move || {
                    media::read_track(&file).map(|info| info.metadata).ok()
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
                let mut entry = Entry {
                    path: key.clone(),
                    title: file_title(&path),
                    ..Entry::default()
                };
                fill_entry(&mut entry, &metadata);
                entry
            }
        };
        lock(&self.0.queue).update_entries(&key, |queued| {
            queued.title.clone_from(&entry.title);
            queued.artist.clone_from(&entry.artist);
            queued.album.clone_from(&entry.album);
            queued.cover.clone_from(&entry.cover);
        });
        self.emit_queue();

        let current = {
            let mut state = lock(&self.0.state);
            state.track.as_mut().filter(|t| t.path == key).map(|track| {
                track.metadata = TrackMetadata::default();
                track.title.clone_from(&entry.title);
                track.id
            })
        };
        if let Some(id) = current {
            self.emit_state();
            self.read_metadata(id, path);
        }
    }

    pub async fn play(&self) -> Result<(), AppError> {
        let status = lock(&self.0.state).status;
        if status == Status::Idle {
            // The queue ended (or nothing was loaded yet): play the current item again.
            let _transport = self.0.transport.lock().await;
            let item = lock(&self.0.queue).current().cloned();
            return match item {
                Some(item) => self.load(&item).await,
                None => Ok(()),
            };
        }
        self.set_pause(false).await
    }

    pub async fn pause(&self) -> Result<(), AppError> {
        self.set_pause(true).await
    }

    /// Pauses or resumes, the track fading out too while a crossfade runs.
    pub(super) async fn set_pause(&self, pause: bool) -> Result<(), AppError> {
        let outgoing = lock(&self.0.fade).outgoing;
        if let Some(client) = outgoing.and_then(|deck| self.deck_client(deck)) {
            let _ = command(
                &client,
                vec![json!("set_property"), json!("pause"), json!(pause)],
            )
            .await;
        }
        self.set_property("pause", json!(pause)).await
    }

    pub async fn toggle(&self) -> Result<(), AppError> {
        let playing = matches!(
            lock(&self.0.state).status,
            Status::Playing | Status::Loading
        );
        if playing {
            self.pause().await
        } else {
            self.play().await
        }
    }

    pub async fn seek(&self, position: f64) -> Result<(), AppError> {
        let position = position.max(0.0);
        // mpv can't seek a file it is still opening (a link waits for yt-dlp): seek once it
        // is loaded.
        let loading = {
            let mut state = lock(&self.0.state);
            let loading = state.status == Status::Loading && state.track.is_some();
            if loading {
                state.position = position;
            }
            loading
        };
        if loading {
            *lock(&self.0.pending_seek) = Some(position);
            self.emit_position(true);
            return Ok(());
        }
        let client = self.client()?;
        // A seek ends a crossfade at once.
        self.end_fade().await;
        command(
            &client,
            vec![json!("seek"), json!(position), json!("absolute+exact")],
        )
        .await?;
        lock(&self.0.state).position = position;
        self.emit_position(true);
        self.follow_lyrics();
        Ok(())
    }

    pub async fn set_volume(&self, volume: f64) -> Result<(), AppError> {
        let volume = volume.clamp(0.0, 100.0);
        if lock(&self.0.fade).ramping {
            // The ramp sets the decks' volumes from this as it goes.
            lock(&self.0.state).volume = volume;
            self.emit_state();
            return Ok(());
        }
        // Only the playing deck: the other waits silent with the next track, and takes the
        // volume when it starts.
        self.set_property("volume", json!(volume)).await
    }

    pub async fn set_muted(&self, muted: bool) -> Result<(), AppError> {
        self.set_on_every_deck("mute", json!(muted)).await
    }

    pub async fn set_speed(&self, speed: f64) -> Result<(), AppError> {
        self.set_on_every_deck("speed", json!(speed.clamp(0.25, 4.0)))
            .await
    }
}

/// A shuffle seed; any changing value will do.
fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x5EED, |d| d.as_nanos() as u64)
}

/// Fills an entry's display fields from tags.
pub(super) fn fill_entry(entry: &mut Entry, metadata: &TrackMetadata) {
    if let Some(title) = &metadata.title {
        entry.title.clone_from(title);
    }
    entry.artist = metadata.artist.clone().or(entry.artist.take());
    entry.album = metadata.album.clone().or(entry.album.take());
    entry.duration = metadata.duration.or(entry.duration);
}

/// Whether a file opened or dropped directly can be played: audio, plus the video containers
/// the library scanner skips (mpv plays their audio).
fn playable(path: &Path) -> bool {
    library::is_audio_file(path)
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "webm"))
}

/// Files to play for opened or dropped paths: playable files as they are, folders as the
/// audio files inside them (recursively, sorted by path).
fn expand(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found = Vec::new();
            let mut stack = vec![path];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    let hidden = entry.file_name().to_string_lossy().starts_with('.');
                    if hidden {
                        continue;
                    }
                    if entry.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(path);
                    } else if library::is_audio_file(&path) {
                        found.push(path);
                    }
                }
            }
            found.sort();
            files.extend(found);
        } else if path.is_file() && playable(&path) {
            files.push(path);
        }
        if files.len() >= MAX_OPEN {
            files.truncate(MAX_OPEN);
            break;
        }
    }
    files
}

/// The file name without extension, shown until (or unless) tags provide a title.
fn file_title(path: &Path) -> String {
    path.file_stem()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_fall_back_to_the_file_name() {
        assert_eq!(
            file_title(Path::new("/music/01 - Intro.flac")),
            "01 - Intro"
        );
        assert_eq!(file_title(Path::new("noext")), "noext");
    }

    #[test]
    fn opening_a_folder_queues_its_audio_files_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let album = dir.path().join("Album");
        std::fs::create_dir_all(album.join("CD2")).unwrap();
        std::fs::create_dir_all(album.join(".hidden")).unwrap();
        for name in [
            "02.flac",
            "01.flac",
            "cover.jpg",
            "CD2/01.mp3",
            ".hidden/x.mp3",
        ] {
            std::fs::write(album.join(name), b"").unwrap();
        }
        let loose = dir.path().join("loose.ogg");
        std::fs::write(&loose, b"").unwrap();

        let files = expand(vec![
            loose.clone(),
            album.clone(),
            dir.path().join("gone.mp3"),
        ]);
        assert_eq!(
            files,
            [
                loose,
                album.join("01.flac"),
                album.join("02.flac"),
                album.join("CD2/01.mp3"),
            ]
        );
    }

    #[test]
    fn entries_take_tags_but_keep_what_tags_lack() {
        let mut entry = Entry {
            path: "/a.mp3".into(),
            title: "a".into(),
            album: Some("Known".into()),
            ..Entry::default()
        };
        fill_entry(
            &mut entry,
            &TrackMetadata {
                title: Some("Song".into()),
                artist: Some("Artist".into()),
                duration: Some(10.0),
                ..TrackMetadata::default()
            },
        );
        assert_eq!(entry.title, "Song");
        assert_eq!(entry.artist.as_deref(), Some("Artist"));
        assert_eq!(entry.album.as_deref(), Some("Known"));
        assert_eq!(entry.duration, Some(10.0));
    }
}
