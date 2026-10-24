//! Playlists imported from other services and files (stage 9): their entries, as the
//! source described them, wait in place until a track is found for them in the library or
//! on YouTube Music (the app's matcher), and keep the alternatives for the user to pick.

use import::ImportedTrack;
use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};

use crate::artists::splitting;
use crate::model::{Playlist, RemoteTrack, Track};
use crate::playlists::{Item, items, write_places};
use crate::remote::store_remote;
use crate::tracks::prune;
use crate::{Error, Library, Result, now};

/// How the search for an entry went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryState {
    /// Not looked for yet.
    Pending,
    /// Found without doubt.
    Sure,
    /// Found, but the user may want another one (shown with a "?").
    Doubtful,
    /// Nothing fitting was found.
    Missing,
    /// The user chose the track.
    Picked,
}

impl EntryState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sure => "sure",
            Self::Doubtful => "doubtful",
            Self::Missing => "missing",
            Self::Picked => "picked",
        }
    }

    fn parse(text: &str) -> Self {
        match text {
            "sure" => Self::Sure,
            "doubtful" => Self::Doubtful,
            "missing" => Self::Missing,
            "picked" => Self::Picked,
            _ => Self::Pending,
        }
    }
}

/// A possible track for an entry: one in the library or a YouTube Music link.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EntryCandidate {
    /// A library track, when the candidate is one.
    pub track_id: Option<i64>,
    /// The link to play, for a candidate from a search.
    pub url: Option<String>,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration: Option<f64>,
    pub score: f64,
}

/// An imported entry and its search.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportEntry {
    pub id: i64,
    pub playlist_id: i64,
    pub source: ImportedTrack,
    pub state: EntryState,
    /// Best first.
    pub candidates: Vec<EntryCandidate>,
    /// The track it was matched to.
    pub track_id: Option<i64>,
}

/// One place of a playlist as its page shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistPlace {
    pub position: usize,
    /// The track, if there is one yet.
    pub track: Option<Track>,
    /// What the import said, for an imported place.
    pub entry: Option<ImportEntry>,
}

/// How far a playlist's import got.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    /// Imported entries.
    pub total: i64,
    /// Found (sure, doubtful or picked).
    pub found: i64,
    pub doubtful: i64,
    pub missing: i64,
    /// Still to look for.
    pub pending: i64,
}

/// An entry to add: what the source said, and the library track it already matches (found
/// by path, ISRC or name at import time), with how sure that is.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEntry {
    pub source: ImportedTrack,
    pub track_id: Option<i64>,
    pub state: EntryState,
    pub candidates: Vec<EntryCandidate>,
}

impl NewEntry {
    pub fn pending(source: ImportedTrack) -> Self {
        Self {
            source,
            track_id: None,
            state: EntryState::Pending,
            candidates: Vec::new(),
        }
    }
}

/// What identifies an entry across imports of the same list: its link on the service, else
/// its names.
fn identity(track: &ImportedTrack) -> String {
    if let Some(url) = &track.url {
        return url.clone();
    }
    let artists = track.artists.join(", ");
    format!(
        "{}\u{1f}{}",
        import::normalize::fold(&artists),
        import::normalize::fold(&track.title)
    )
}

fn insert_entry(tx: &Transaction<'_>, playlist_id: i64, entry: &NewEntry) -> Result<i64> {
    let source = &entry.source;
    tx.execute(
        "INSERT INTO import_entries (playlist_id, title, artists, album, duration, isrc, url,
             path, swappable, state, candidates, checked_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            playlist_id,
            source.title,
            serde_json::to_string(&source.artists)?,
            source.album,
            source.duration,
            source.isrc,
            source.url,
            source.path,
            source.swappable,
            entry.state.as_str(),
            serde_json::to_string(&entry.candidates)?,
            (entry.state != EntryState::Pending).then(now),
        ],
    )?;
    Ok(tx.last_insert_rowid())
}

const ENTRY_COLUMNS: &str = "e.id, e.playlist_id, e.title, e.artists, e.album, e.duration,
    e.isrc, e.url, e.path, e.swappable, e.state, e.candidates,
    (SELECT track_id FROM playlist_items WHERE entry_id = e.id)";

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImportEntry> {
    let artists: String = row.get(3)?;
    let candidates: String = row.get(11)?;
    let state: String = row.get(10)?;
    Ok(ImportEntry {
        id: row.get(0)?,
        playlist_id: row.get(1)?,
        source: ImportedTrack {
            title: row.get(2)?,
            artists: serde_json::from_str(&artists).unwrap_or_default(),
            album: row.get(4)?,
            duration: row.get(5)?,
            isrc: row.get(6)?,
            url: row.get(7)?,
            path: row.get(8)?,
            swappable: row.get(9)?,
        },
        state: EntryState::parse(&state),
        candidates: serde_json::from_str(&candidates).unwrap_or_default(),
        track_id: row.get(12)?,
    })
}

impl Library {
    /// Makes a playlist of imported entries, in order; with `source_url` (a service's
    /// link), importing it again goes to [`Self::reimport_playlist`].
    pub fn create_imported_playlist(
        &mut self,
        name: &str,
        source_url: Option<&str>,
        entries: &[NewEntry],
    ) -> Result<Playlist> {
        let tx = self.conn.transaction()?;
        let at = now();
        let name = name.trim();
        let name = if name.is_empty() {
            source_url.unwrap_or("Imported")
        } else {
            name
        };
        tx.execute(
            "INSERT INTO playlists (name, created_at, updated_at, source_url)
             VALUES (?1, ?2, ?2, ?3)",
            params![name, at, source_url],
        )?;
        let playlist_id = tx.last_insert_rowid();
        let mut places = Vec::with_capacity(entries.len());
        for entry in entries {
            let entry_id = insert_entry(&tx, playlist_id, entry)?;
            places.push(Item {
                track_id: entry.track_id,
                entry_id: Some(entry_id),
            });
        }
        write_places(&tx, playlist_id, &places)?;
        tx.commit()?;
        self.playlist(playlist_id)?
            .ok_or_else(|| Error::Invalid("playlist vanished".into()))
    }

    /// Brings an imported playlist up to the source's current list: entries it still has
    /// keep their tracks and the user's picks, new ones wait to be looked for, gone ones
    /// leave. Tracks the user added themselves stay, after the source's. A `partial` list
    /// (the first part of a longer one) removes nothing: entries past it stay after it.
    pub fn reimport_playlist(
        &mut self,
        playlist_id: i64,
        entries: &[NewEntry],
        partial: bool,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let places = items(&tx, playlist_id)?;
        let mut known: std::collections::HashMap<String, Vec<Item>> =
            std::collections::HashMap::new();
        // The source's places in their order, for a partial list.
        let mut sourced: Vec<(String, Item)> = Vec::new();
        let mut own: Vec<Item> = Vec::new();
        {
            let mut statement = tx.prepare(&format!(
                "SELECT {ENTRY_COLUMNS} FROM import_entries e WHERE e.id = ?"
            ))?;
            for place in &places {
                match place.entry_id {
                    Some(entry_id) => {
                        let entry = statement.query_row([entry_id], entry_from_row).optional()?;
                        if let Some(entry) = entry {
                            let id = identity(&entry.source);
                            known.entry(id.clone()).or_default().push(*place);
                            sourced.push((id, *place));
                        }
                    }
                    None => own.push(*place),
                }
            }
        }
        let mut next = Vec::with_capacity(entries.len() + own.len());
        for entry in entries {
            let kept = known
                .get_mut(&identity(&entry.source))
                .and_then(|list| (!list.is_empty()).then(|| list.remove(0)));
            match kept {
                Some(place) => next.push(place),
                None => {
                    let entry_id = insert_entry(&tx, playlist_id, entry)?;
                    next.push(Item {
                        track_id: entry.track_id,
                        entry_id: Some(entry_id),
                    });
                }
            }
        }
        if partial {
            // What the list didn't show this time: every place not taken above, in order.
            for (id, place) in sourced {
                if let Some(list) = known.get_mut(&id)
                    && let Some(index) = list.iter().position(|p| *p == place)
                {
                    next.push(list.remove(index));
                }
            }
        }
        next.extend(own);
        write_places(&tx, playlist_id, &next)?;
        tx.commit()?;
        Ok(())
    }

    /// The imported playlist that came from `url`.
    pub fn imported_playlist(&self, url: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT p.id FROM playlists p WHERE p.source_url = ?1
                     AND EXISTS (SELECT 1 FROM import_entries e WHERE e.playlist_id = p.id)
                 ORDER BY p.id LIMIT 1",
                [url],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// Entries still to look for, oldest playlists first, in each playlist's order.
    pub fn pending_entries(&self, limit: usize) -> Result<Vec<ImportEntry>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {ENTRY_COLUMNS} FROM import_entries e
             JOIN playlist_items pi ON pi.entry_id = e.id
             WHERE e.state = 'pending'
             ORDER BY e.playlist_id, pi.position LIMIT ?"
        ))?;
        let entries = statement
            .query_map([limit as i64], entry_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(entries)
    }

    pub fn import_entry(&self, id: i64) -> Result<Option<ImportEntry>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM import_entries e WHERE e.id = ?"),
                [id],
                entry_from_row,
            )
            .optional()?)
    }

    /// Records the search for an entry: its state, the alternatives, and the track its
    /// place now plays (`None` leaves it waiting, greyed out).
    pub fn resolve_entry(
        &mut self,
        entry_id: i64,
        state: EntryState,
        track_id: Option<i64>,
        candidates: &[EntryCandidate],
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE import_entries SET state = ?2, candidates = ?3, checked_at = ?4 WHERE id = ?1",
            params![
                entry_id,
                state.as_str(),
                serde_json::to_string(candidates)?,
                now()
            ],
        )?;
        tx.execute(
            "UPDATE playlist_items SET track_id = ?2 WHERE entry_id = ?1",
            params![entry_id, track_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Fills in what the source says about an entry that came as a bare link (Spotify
    /// tracks pasted in bulk) once its page was read.
    pub fn update_entry_source(&self, entry_id: i64, source: &ImportedTrack) -> Result<()> {
        self.conn.execute(
            "UPDATE import_entries SET title = ?2, artists = ?3, album = ?4, duration = ?5,
                 isrc = COALESCE(?6, isrc)
             WHERE id = ?1",
            params![
                entry_id,
                source.title,
                serde_json::to_string(&source.artists)?,
                source.album,
                source.duration,
                source.isrc,
            ],
        )?;
        Ok(())
    }

    /// A link found for an entry, as a library track named the way the import named it
    /// (a service's titles are cleaner than video titles); a link the library already has
    /// stays as it is. Marked as corrected, so yt-dlp's later descriptions keep the names.
    pub fn add_found_link(&mut self, url: &str, source: &ImportedTrack) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let known: Option<i64> = tx
            .query_row(
                "SELECT id FROM tracks WHERE source_url = ? ORDER BY path = source_url LIMIT 1",
                [url],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = known {
            return Ok(id);
        }
        let splitting = splitting(&tx)?;
        let remote = RemoteTrack {
            url: url.to_owned(),
            title: source.title.clone(),
            artist: (source.artists.len() == 1).then(|| source.artists[0].clone()),
            artists: if source.artists.len() > 1 {
                source.artists.clone()
            } else {
                Vec::new()
            },
            album: source.album.clone(),
            duration: source.duration,
            complete: true,
            ..RemoteTrack::default()
        };
        let id = store_remote(&tx, &remote, now(), &splitting)?;
        tx.execute("UPDATE tracks SET edited = 1 WHERE id = ?", [id])?;
        prune(&tx)?;
        tx.commit()?;
        Ok(id)
    }

    /// Every place of a playlist, imported entries still being looked for included.
    pub fn playlist_places(&self, playlist_id: i64) -> Result<Vec<PlaylistPlace>> {
        let rows: Vec<(i64, Option<i64>, Option<i64>)> = {
            let mut statement = self.conn.prepare(
                "SELECT position, track_id, entry_id FROM playlist_items
                 WHERE playlist_id = ? ORDER BY position",
            )?;
            statement
                .query_map([playlist_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?
                .collect::<rusqlite::Result<_>>()?
        };
        let track_ids: Vec<i64> = rows.iter().filter_map(|(_, id, _)| *id).collect();
        let tracks: std::collections::HashMap<i64, Track> = self
            .tracks(&crate::TrackFilter::Ids(track_ids))?
            .into_iter()
            .map(|track| (track.id, track))
            .collect();
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT {ENTRY_COLUMNS} FROM import_entries e WHERE e.id = ?"
        ))?;
        rows.into_iter()
            .enumerate()
            .map(|(position, (_, track_id, entry_id))| {
                let entry = match entry_id {
                    Some(id) => statement.query_row([id], entry_from_row).optional()?,
                    None => None,
                };
                Ok(PlaylistPlace {
                    position,
                    track: track_id.and_then(|id| tracks.get(&id).cloned()),
                    entry,
                })
            })
            .collect()
    }

    /// How far each imported playlist got; playlists without entries aren't listed.
    pub fn import_progress(&self, playlist_id: i64) -> Result<ImportProgress> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*),
                 SUM(state IN ('sure', 'doubtful', 'picked')),
                 SUM(state = 'doubtful'),
                 SUM(state = 'missing'),
                 SUM(state = 'pending')
             FROM import_entries WHERE playlist_id = ?",
            [playlist_id],
            |row| {
                Ok(ImportProgress {
                    total: row.get(0)?,
                    found: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    doubtful: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    missing: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    pending: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                })
            },
        )?)
    }

    /// Library tracks that might be `wanted`, for the caller to score: the file it names,
    /// else tracks sharing the words of its title and artist (or of its title alone, when
    /// the artist is spelled another way), at most `limit`.
    pub fn library_candidates(&self, wanted: &ImportedTrack, limit: usize) -> Result<Vec<Track>> {
        if let Some(path) = &wanted.path
            && let Some(track) = self.track_by_path(path)?
        {
            return Ok(vec![track]);
        }
        let title = import::normalize::title(&wanted.title).base;
        let words = |text: &str, most: usize| -> String {
            text.split(' ')
                .filter(|w| !w.is_empty())
                .take(most)
                .collect::<Vec<_>>()
                .join(" ")
        };
        let title = words(&title, 6);
        if title.is_empty() {
            return Ok(Vec::new());
        }
        let artist = wanted
            .artists
            .first()
            .map(|a| words(&import::normalize::fold(a), 3))
            .unwrap_or_default();
        let mut ids = if artist.is_empty() {
            Vec::new()
        } else {
            self.search_track_ids(&format!("{title} {artist}"))?
        };
        if ids.is_empty() {
            ids = self.search_track_ids(&title)?;
        }
        ids.truncate(limit);
        self.tracks(&crate::TrackFilter::Ids(ids))
    }

    /// Puts entries back in the queue to be looked for again (a pick undone, or "Find again"
    /// for those that weren't found).
    pub fn retry_entries(&mut self, playlist_id: i64, only_missing: bool) -> Result<usize> {
        let changed = self.conn.execute(
            if only_missing {
                "UPDATE import_entries SET state = 'pending' WHERE playlist_id = ? AND state = 'missing'"
            } else {
                "UPDATE import_entries SET state = 'pending'
                 WHERE playlist_id = ? AND state IN ('missing', 'doubtful', 'sure')"
            },
            [playlist_id],
        )?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackFilter;
    use crate::tracks::tests::sample_library;

    fn imported(title: &str, artist: &str) -> ImportedTrack {
        ImportedTrack::new(title, vec![artist.into()])
    }

    #[test]
    fn imported_entries_wait_in_place() {
        let mut library = sample_library();
        let first = library
            .tracks(&TrackFilter::All)
            .unwrap()
            .into_iter()
            .find(|t| t.title == "First")
            .unwrap();
        let entries = [
            NewEntry {
                state: EntryState::Sure,
                track_id: Some(first.id),
                ..NewEntry::pending(imported("First", "Someone"))
            },
            NewEntry::pending(imported("Not Here", "Nobody")),
        ];
        let playlist = library
            .create_imported_playlist(
                "From Spotify",
                Some("https://open.spotify.com/playlist/x"),
                &entries,
            )
            .unwrap();
        // Only found tracks play and count.
        assert_eq!(playlist.track_count, 1);
        let tracks = library.tracks(&TrackFilter::Playlist(playlist.id)).unwrap();
        assert_eq!(tracks.len(), 1);
        let places = library.playlist_places(playlist.id).unwrap();
        assert_eq!(places.len(), 2);
        assert_eq!(places[0].track.as_ref().map(|t| t.id), Some(first.id));
        assert!(places[1].track.is_none());
        assert_eq!(places[1].entry.as_ref().unwrap().source.title, "Not Here");

        let progress = library.import_progress(playlist.id).unwrap();
        assert_eq!(
            (progress.total, progress.found, progress.pending),
            (2, 1, 1)
        );

        // The matcher finds the second one on YouTube Music.
        let pending = library.pending_entries(10).unwrap();
        assert_eq!(pending.len(), 1);
        let entry = &pending[0];
        let id = library
            .add_found_link("https://music.youtube.com/watch?v=abc", &entry.source)
            .unwrap();
        library
            .resolve_entry(entry.id, EntryState::Doubtful, Some(id), &[])
            .unwrap();
        let tracks = library.tracks(&TrackFilter::Playlist(playlist.id)).unwrap();
        assert_eq!(tracks[1].title, "Not Here");
        assert_eq!(tracks[1].artist.as_deref(), Some("Nobody"));
        assert!(library.pending_entries(10).unwrap().is_empty());
        assert_eq!(library.import_progress(playlist.id).unwrap().doubtful, 1);
        assert_eq!(
            library
                .imported_playlist("https://open.spotify.com/playlist/x")
                .unwrap(),
            Some(playlist.id)
        );
    }

    #[test]
    fn finds_library_candidates() {
        let library = sample_library();
        let all = library.tracks(&TrackFilter::All).unwrap();
        let first = all.iter().find(|t| t.title == "First").unwrap();
        // By the file it names.
        let mut by_path = imported("Whatever", "Anyone");
        by_path.path = Some(first.path.clone());
        assert_eq!(
            library.library_candidates(&by_path, 5).unwrap()[0].id,
            first.id
        );
        // By name, versions' noise aside.
        let found = library
            .library_candidates(&imported("First - Remastered 2011", "Nobody Known"), 5)
            .unwrap();
        assert!(found.iter().any(|t| t.id == first.id));
        assert!(
            library
                .library_candidates(&imported("Zzz", "x"), 5)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn editing_keeps_entries_with_their_places() {
        let mut library = sample_library();
        let playlist = library
            .create_imported_playlist(
                "Text",
                None,
                &[
                    NewEntry::pending(imported("A", "x")),
                    NewEntry::pending(imported("B", "x")),
                    NewEntry::pending(imported("C", "x")),
                ],
            )
            .unwrap();
        library.move_in_playlist(playlist.id, 0, 2).unwrap();
        library.remove_from_playlist(playlist.id, &[0]).unwrap();
        let titles: Vec<String> = library
            .playlist_places(playlist.id)
            .unwrap()
            .into_iter()
            .map(|place| place.entry.unwrap().source.title)
            .collect();
        assert_eq!(titles, ["C", "A"]);
        // The removed entry is gone, not orphaned.
        assert_eq!(library.import_progress(playlist.id).unwrap().total, 2);
    }

    #[test]
    fn importing_again_keeps_what_was_found_and_picked() {
        let mut library = sample_library();
        let first = library.tracks(&TrackFilter::All).unwrap()[0].id;
        let mut a = imported("A", "x");
        a.url = Some("https://open.spotify.com/track/a".into());
        let b = imported("B", "x");
        let playlist = library
            .create_imported_playlist(
                "Mix",
                Some("https://open.spotify.com/playlist/m"),
                &[NewEntry::pending(a.clone()), NewEntry::pending(b.clone())],
            )
            .unwrap();
        let entries = library.pending_entries(10).unwrap();
        library
            .resolve_entry(entries[0].id, EntryState::Picked, Some(first), &[])
            .unwrap();
        library
            .add_to_playlist(playlist.id, &[first], None)
            .unwrap();
        // The source lost B and gained C, and A moved down.
        let c = imported("C", "y");
        library
            .reimport_playlist(
                playlist.id,
                &[NewEntry::pending(c.clone()), NewEntry::pending(a.clone())],
                false,
            )
            .unwrap();
        let places = library.playlist_places(playlist.id).unwrap();
        let names: Vec<String> = places
            .iter()
            .map(|p| match &p.entry {
                Some(entry) => entry.source.title.clone(),
                None => "own".into(),
            })
            .collect();
        assert_eq!(names, ["C", "A", "own"]);
        assert_eq!(places[1].entry.as_ref().unwrap().state, EntryState::Picked);
        assert_eq!(places[1].track.as_ref().map(|t| t.id), Some(first));
        assert_eq!(library.import_progress(playlist.id).unwrap().total, 2);

        // Only the first part of the list (Spotify's page): nothing past it goes.
        let d = imported("D", "z");
        library
            .reimport_playlist(playlist.id, &[NewEntry::pending(d)], true)
            .unwrap();
        let names: Vec<String> = library
            .playlist_places(playlist.id)
            .unwrap()
            .iter()
            .map(|p| match &p.entry {
                Some(entry) => entry.source.title.clone(),
                None => "own".into(),
            })
            .collect();
        assert_eq!(names, ["D", "C", "A", "own"]);
    }
}
