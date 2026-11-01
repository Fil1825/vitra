//! Tags and artwork for the playing track, read off the async runtime so a large
//! file doesn't stall the player.

use std::path::PathBuf;

use media::TrackMetadata;
use tauri::Manager;

use crate::links::LinkService;
use crate::lyrics::TrackInfo;

use super::playback::fill_entry;
use super::state::ArtworkInfo;
use super::{Player, TRACK_EVENT, lock};

impl Player {
    /// Reads tags and artwork off the async runtime, then fills them into the track if it is
    /// still the current one.
    pub(super) fn read_metadata(&self, id: u64, path: PathBuf) {
        let player = self.clone();
        tauri::async_runtime::spawn(async move {
            // The cover the user chose for the album comes before the file's artwork.
            let custom = player.custom_cover(&path.to_string_lossy()).await;
            let link = path.to_str().filter(|path| links::is_remote(path));
            let thumbnail = match link {
                Some(url) if custom.is_none() => player.link_thumbnail(url).await,
                _ => None,
            };
            let remote = link.is_some();
            let result = tauri::async_runtime::spawn_blocking(move || {
                let custom = custom.map(|bytes| media::Artwork {
                    bytes,
                    mime: Some("image/jpeg".into()),
                });
                // A link has no file to read: its tags come with its queue entry, its
                // artwork is its thumbnail.
                if remote {
                    let artwork = custom
                        .or_else(|| thumbnail.map(|bytes| media::Artwork { bytes, mime: None }))
                        .and_then(|art| {
                            media::prepare_artwork(art)
                                .inspect_err(|err| log::info!("thumbnail of a link: {err}"))
                                .ok()
                        });
                    return (TrackMetadata::default(), artwork);
                }
                let info = media::read_track(&path)
                    .inspect_err(|err| log::info!("tags of {}: {err}", path.display()))
                    .unwrap_or_default();
                let artwork = custom
                    .or(info.embedded_artwork)
                    .or_else(|| media::folder_artwork(&path))
                    .and_then(|art| {
                        media::prepare_artwork(art)
                            .inspect_err(|err| log::info!("artwork of {}: {err}", path.display()))
                            .ok()
                    });
                (info.metadata, artwork)
            })
            .await;
            let Ok((metadata, artwork)) = result else {
                return;
            };

            let artwork = artwork.map(|prepared| {
                let palette = prepared.palette.clone();
                ArtworkInfo {
                    id: player.0.artwork.insert(prepared),
                    palette,
                }
            });
            let track = {
                let mut state = lock(&player.0.state);
                let Some(track) = state.track.as_mut().filter(|t| t.id == id) else {
                    return;
                };
                fill_missing(&mut track.metadata, metadata);
                if let Some(title) = &track.metadata.title {
                    track.title.clone_from(title);
                }
                track.artwork = artwork;
                track.clone()
            };
            let queued = lock(&player.0.queue).update_entries(&track.path, |entry| {
                if entry.track_id.is_none() {
                    fill_entry(entry, &track.metadata);
                }
            });
            if queued {
                player.queue_changed_quietly();
            }
            player.emit_state();
            player.emit(TRACK_EVENT, &track);
            if let Some(lyrics) = player.lyrics() {
                lyrics.track_ready(TrackInfo {
                    id: track.id,
                    path: track.path,
                    library_id: track.library_id,
                    title: track.title,
                    metadata: track.metadata,
                });
            }
        });
    }

    /// The cover the user chose for the album of the track at `path`.
    pub(super) async fn custom_cover(&self, path: &str) -> Option<Vec<u8>> {
        let library = self.library()?;
        let key = path.to_owned();
        let covers = library.covers_dir().to_path_buf();
        library
            .run(move |db| {
                let cover = db.custom_album_cover(&key)?;
                Ok(cover.and_then(|id| std::fs::read(covers.join(format!("{id}.jpg"))).ok()))
            })
            .await
            .ok()
            .flatten()
    }

    /// A link's artwork: its album's cover if the user put it in an album, else its thumbnail
    /// from a recent resolution (full size), else the library's cached cover.
    pub(super) async fn link_thumbnail(&self, url: &str) -> Option<Vec<u8>> {
        let recent = self
            .0
            .app
            .try_state::<LinkService>()
            .and_then(|links| links.thumbnail(url));
        let stored = match self.library() {
            Some(library) => {
                let url = url.to_owned();
                let covers = library.covers_dir().to_path_buf();
                let wanted = recent.is_none();
                library
                    .run(move |db| {
                        let Some(track) = db.track_by_path(&url)? else {
                            return Ok(None);
                        };
                        // The library gives a corrected link in an album its album's cover.
                        let albums = track.album_id.is_some() && db.link_edited(&url)?;
                        if !albums && !wanted {
                            return Ok(None);
                        }
                        let read = |id: String| std::fs::read(covers.join(format!("{id}.jpg")));
                        Ok(track
                            .cover
                            .and_then(|id| read(id).ok())
                            .map(|bytes| (bytes, albums)))
                    })
                    .await
                    .ok()
                    .flatten()
            }
            None => None,
        };
        match (stored, recent) {
            (Some((bytes, true)), _) => Some(bytes),
            (_, Some(recent)) => Some(recent.as_ref().clone()),
            (stored, None) => stored.map(|(bytes, _)| bytes),
        }
    }
}

/// Copies every field `target` doesn't have yet from `source`.
pub(super) fn fill_missing(target: &mut TrackMetadata, source: TrackMetadata) {
    macro_rules! fill {
        ($($field:ident),*) => { $( if target.$field.is_none() { target.$field = source.$field; } )* };
    }
    fill!(
        title,
        artist,
        album,
        album_artist,
        genre,
        year,
        track_number,
        disc_number,
        duration,
        format,
        sample_rate,
        bit_depth,
        bitrate,
        channels
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_missing_keeps_existing_values() {
        let mut target = TrackMetadata {
            title: Some("From tags".into()),
            ..TrackMetadata::default()
        };
        fill_missing(
            &mut target,
            TrackMetadata {
                title: Some("From mpv".into()),
                artist: Some("Artist".into()),
                ..TrackMetadata::default()
            },
        );
        assert_eq!(target.title.as_deref(), Some("From tags"));
        assert_eq!(target.artist.as_deref(), Some("Artist"));
    }
}
