//! What My Wave keeps in the library (migration 10): each track's measured sound, which
//! tracks still need measuring, and the listener's skips and dislikes; plus the candidates
//! and habits the wave picks from.

use std::collections::{HashMap, HashSet};

use mood::{Candidate, Features, Habits};
use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{FileStamp, Library, Result};

/// Links are tracks whose path is their page's address.
const IS_LINK: &str = "(t.path LIKE 'http://%' OR t.path LIKE 'https://%')";
/// Best first: favorites, then what was played lately, then most, then newest.
const LIKELY_FIRST: &str = "t.favorite DESC, t.last_played_at IS NULL, t.last_played_at DESC, \
                            t.play_count DESC, t.added_at DESC, t.id";
/// A skip counts less as it ages: after a month, a third.
const SKIP_FADE_SECONDS: f64 = 30.0 * 24.0 * 3600.0;
/// Plays this close together count as one after the other.
const SEQUENCE_GAP_SECONDS: i64 = 30 * 60;
/// How much play history the wave learns sequences from.
const HISTORY_ROWS: i64 = 5000;

/// A track to measure: a file (with what the library knows of its length and stamp) or a
/// link.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalysisJob {
    pub track_id: i64,
    pub path: String,
    pub title: String,
    pub duration: Option<f64>,
    pub stamp: Option<FileStamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureSource {
    File,
    Stream,
}

impl FeatureSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Stream => "stream",
        }
    }
}

/// How far the analysis has come for the current version.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProgress {
    pub files_done: u32,
    pub files_total: u32,
    pub links_done: u32,
    pub links_total: u32,
    /// Tracks that couldn't be measured (counted in the done ones).
    pub failed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// Skipped early.
    Skip { in_wave: bool },
    /// "Don't like": the wave never picks it again (until it is liked).
    Dislike,
}

/// A track the wave may pick, with its path (a link plays only online).
#[derive(Debug, Clone, PartialEq)]
pub struct WaveCandidate {
    pub candidate: Candidate,
    pub path: String,
}

impl WaveCandidate {
    pub fn is_link(&self) -> bool {
        self.path.starts_with("http://") || self.path.starts_with("https://")
    }
}

impl Library {
    /// Files still to measure for `version` (never measured, measured by an older version, or
    /// changed since), the likeliest to be wanted first. A failure for this version and file
    /// isn't tried again.
    pub fn files_to_analyze(&self, version: i64, limit: i64) -> Result<Vec<AnalysisJob>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT t.id, t.path, t.duration, t.file_size, t.file_mtime, t.title
             FROM tracks t LEFT JOIN track_features f ON f.track_id = t.id
             WHERE NOT {IS_LINK}
               AND (f.track_id IS NULL OR f.version < ?1
                    OR f.file_size IS NOT t.file_size OR f.file_mtime IS NOT t.file_mtime)
             ORDER BY {LIKELY_FIRST} LIMIT ?2"
        ))?;
        let jobs = statement
            .query_map(params![version, limit], |row| {
                Ok(AnalysisJob {
                    track_id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(5)?,
                    duration: row.get(2)?,
                    stamp: Some((row.get(3)?, row.get(4)?)),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(jobs)
    }

    /// Links still to measure for `version`, the likeliest first. Decoder failures may be
    /// environmental (TLS, a sidecar update); retry after five minutes, not in a tight loop.
    pub fn links_to_analyze(&self, version: i64, limit: i64) -> Result<Vec<AnalysisJob>> {
        let mut statement = self.conn.prepare_cached(&format!(
            "SELECT t.id, t.path, t.duration, t.title
             FROM tracks t LEFT JOIN track_features f ON f.track_id = t.id
             WHERE {IS_LINK} AND (f.track_id IS NULL OR f.version < ?1
                 OR (f.error LIKE 'FFmpeg failed:%' AND f.analyzed_at <= ?3))
             ORDER BY {LIKELY_FIRST} LIMIT ?2"
        ))?;
        let jobs = statement
            .query_map(params![version, limit, crate::now() - 300], |row| {
                Ok(AnalysisJob {
                    track_id: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(3)?,
                    duration: row.get(2)?,
                    stamp: None,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(jobs)
    }

    /// A track as a job to measure (the wave asks for some links first).
    pub fn analysis_job(&self, track_id: i64) -> Result<Option<AnalysisJob>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, path, duration, file_size, file_mtime, title FROM tracks WHERE id = ?1",
                [track_id],
                |row| {
                    let path: String = row.get(1)?;
                    let link = path.starts_with("http://") || path.starts_with("https://");
                    Ok(AnalysisJob {
                        track_id: row.get(0)?,
                        title: row.get(5)?,
                        duration: row.get(2)?,
                        stamp: if link {
                            None
                        } else {
                            Some((row.get(3)?, row.get(4)?))
                        },
                        path,
                    })
                },
            )
            .optional()?)
    }

    /// Stores a measurement, or why there is none.
    pub fn set_features(
        &self,
        track_id: i64,
        version: i64,
        source: FeatureSource,
        stamp: Option<FileStamp>,
        measured: std::result::Result<Features, String>,
        at: i64,
    ) -> Result<()> {
        let (features, error) = match measured {
            Ok(features) => (Some(features), None),
            Err(error) => (None, Some(error)),
        };
        self.conn.execute(
            "INSERT OR REPLACE INTO track_features (track_id, version, source, file_size,
                 file_mtime, energy, spread, tempo, tempo_confidence, brightness, valence,
                 error, analyzed_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
             WHERE EXISTS (SELECT 1 FROM tracks WHERE id = ?1)",
            params![
                track_id,
                version,
                source.as_str(),
                stamp.map(|(size, _)| size),
                stamp.map(|(_, mtime)| mtime),
                features.map(|f| f.energy),
                features.map(|f| f.spread),
                features.map(|f| f.tempo),
                features.map(|f| f.tempo_confidence),
                features.map(|f| f.brightness),
                features.map(|f| f.valence),
                error,
                at,
            ],
        )?;
        Ok(())
    }

    /// A track's measured sound (any version), if it was measured.
    pub fn features(&self, track_id: i64) -> Result<Option<Features>> {
        Ok(self
            .conn
            .query_row(
                "SELECT energy, spread, tempo, tempo_confidence, brightness, valence
                 FROM track_features WHERE track_id = ?1 AND error IS NULL",
                [track_id],
                features_from_row,
            )
            .optional()?)
    }

    pub fn analysis_progress(&self, version: i64) -> Result<AnalysisProgress> {
        Ok(self.conn.query_row(
            &format!(
                "SELECT
                   COALESCE(SUM(NOT {IS_LINK}), 0),
                   COALESCE(SUM(NOT {IS_LINK} AND f.version >= ?1
                       AND (f.source = 'stream' OR (f.file_size IS t.file_size
                            AND f.file_mtime IS t.file_mtime))), 0),
                   COALESCE(SUM({IS_LINK}), 0),
                   COALESCE(SUM({IS_LINK} AND f.version >= ?1), 0),
                   COALESCE(SUM(f.version >= ?1 AND f.error IS NOT NULL), 0)
                 FROM tracks t LEFT JOIN track_features f ON f.track_id = t.id"
            ),
            [version],
            |row| {
                Ok(AnalysisProgress {
                    files_total: row.get(0)?,
                    files_done: row.get(1)?,
                    links_total: row.get(2)?,
                    links_done: row.get(3)?,
                    failed: row.get(4)?,
                })
            },
        )?)
    }

    /// Every measured track as the wave sees it.
    pub fn wave_candidates(&self) -> Result<Vec<WaveCandidate>> {
        let mut artists: HashMap<i64, Vec<i64>> = HashMap::new();
        {
            let mut statement = self.conn.prepare_cached(
                "SELECT track_id, artist_id FROM track_artists ORDER BY track_id, position",
            )?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            for row in rows {
                let (track, artist): (i64, i64) = row?;
                artists.entry(track).or_default().push(artist);
            }
        }
        let mut statement = self.conn.prepare_cached(
            "SELECT f.energy, f.spread, f.tempo, f.tempo_confidence, f.brightness, f.valence,
                    t.id, t.path, t.genre, t.year, t.favorite, t.play_count, t.last_played_at,
                    t.artist_id
             FROM tracks t JOIN track_features f ON f.track_id = t.id
             WHERE f.error IS NULL",
        )?;
        let candidates = statement
            .query_map([], |row| {
                let id: i64 = row.get(6)?;
                let main: Option<i64> = row.get(13)?;
                Ok(WaveCandidate {
                    candidate: Candidate {
                        id,
                        features: features_from_row(row)?,
                        artists: artists
                            .get(&id)
                            .cloned()
                            .unwrap_or_else(|| main.into_iter().collect()),
                        genre: row
                            .get::<_, Option<String>>(8)?
                            .map(|g| g.trim().to_lowercase())
                            .filter(|g| !g.is_empty()),
                        year: row.get(9)?,
                        favorite: row.get(10)?,
                        plays: row.get(11)?,
                        last_played: row.get(12)?,
                    },
                    path: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(candidates)
    }

    /// The listener's skips (faded with age), dislikes and habitual sequences.
    pub fn wave_habits(&self, now: i64) -> Result<Habits> {
        let mut habits = Habits::default();
        {
            let mut statement = self
                .conn
                .prepare_cached("SELECT track_id, kind, marked_at FROM wave_marks")?;
            let rows = statement.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (track, kind, at) = row?;
                if kind == "dislike" {
                    habits.dislikes.insert(track);
                } else {
                    let age = (now - at).max(0) as f64;
                    *habits.skips.entry(track).or_default() +=
                        (-age / SKIP_FADE_SECONDS).exp() as f32;
                }
            }
        }
        let mut statement = self.conn.prepare_cached(
            "SELECT track_id, played_at FROM history ORDER BY played_at DESC, id DESC LIMIT ?1",
        )?;
        let mut plays: Vec<(i64, i64)> = statement
            .query_map([HISTORY_ROWS], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        plays.reverse();
        for pair in plays.windows(2) {
            let ((a, at), (b, bt)) = (pair[0], pair[1]);
            if a != b && bt - at <= SEQUENCE_GAP_SECONDS {
                *habits.transitions.entry((a, b)).or_default() += 1;
            }
        }
        Ok(habits)
    }

    pub fn mark(&self, track_id: i64, mark: Mark, at: i64) -> Result<()> {
        let (kind, in_wave) = match mark {
            Mark::Skip { in_wave } => ("skip", in_wave),
            Mark::Dislike => ("dislike", true),
        };
        self.conn.execute(
            "INSERT INTO wave_marks (track_id, kind, in_wave, marked_at)
             SELECT ?1, ?2, ?3, ?4 WHERE EXISTS (SELECT 1 FROM tracks WHERE id = ?1)",
            params![track_id, kind, in_wave, at],
        )?;
        Ok(())
    }

    /// Tracks the listener said they don't like.
    pub fn disliked(&self) -> Result<HashSet<i64>> {
        let mut statement = self
            .conn
            .prepare_cached("SELECT DISTINCT track_id FROM wave_marks WHERE kind = 'dislike'")?;
        let ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Liking a track takes back an earlier "don't like".
    pub(crate) fn forgive(&self, track_id: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM wave_marks WHERE track_id = ?1 AND kind = 'dislike'",
            [track_id],
        )?;
        Ok(())
    }
}

fn features_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Features> {
    Ok(Features {
        energy: row.get(0)?,
        spread: row.get(1)?,
        tempo: row.get(2)?,
        tempo_confidence: row.get(3)?,
        brightness: row.get(4)?,
        valence: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracks::tests::sample_library;

    fn sound(energy: f32) -> Features {
        Features {
            energy,
            spread: 3.0,
            tempo: 120.0,
            tempo_confidence: 0.8,
            brightness: 1800.0,
            valence: 0.1,
        }
    }

    fn id_of(library: &Library, path: &str) -> i64 {
        library.track_by_path(path).unwrap().unwrap().id
    }

    fn add_link(library: &Library, url: &str) -> i64 {
        library
            .conn
            .execute(
                "INSERT INTO tracks (path, title, added_at, source_url) VALUES (?1, 'Link', 1, ?1)",
                [url],
            )
            .unwrap();
        library.conn.last_insert_rowid()
    }

    #[test]
    fn files_are_measured_once_per_version_and_change() {
        let library = sample_library();
        let all = library.files_to_analyze(1, 100).unwrap();
        assert_eq!(all.len(), 4);
        let job = &all[0];
        assert_eq!(job.stamp, Some((100, 1)));
        assert_eq!(job.duration, Some(200.0));
        library
            .set_features(
                job.track_id,
                1,
                FeatureSource::File,
                job.stamp,
                Ok(sound(-10.0)),
                5,
            )
            .unwrap();
        assert_eq!(library.files_to_analyze(1, 100).unwrap().len(), 3);
        assert_eq!(library.features(job.track_id).unwrap(), Some(sound(-10.0)));
        // A failure isn't retried either...
        let failing = all[1].track_id;
        library
            .set_features(
                failing,
                1,
                FeatureSource::File,
                all[1].stamp,
                Err("bad".into()),
                5,
            )
            .unwrap();
        assert_eq!(library.files_to_analyze(1, 100).unwrap().len(), 2);
        assert_eq!(library.features(failing).unwrap(), None);
        // ...until the analysis improves or the file changes.
        assert_eq!(library.files_to_analyze(2, 100).unwrap().len(), 4);
        library
            .conn
            .execute(
                "UPDATE tracks SET file_mtime = 2 WHERE id = ?1",
                [job.track_id],
            )
            .unwrap();
        assert!(
            library
                .files_to_analyze(1, 100)
                .unwrap()
                .iter()
                .any(|j| j.track_id == job.track_id)
        );
    }

    #[test]
    fn favorites_and_recent_plays_come_first() {
        let mut library = sample_library();
        let favorite = id_of(&library, "/m/c/x.mp3");
        let played = id_of(&library, "/m/a/1.mp3");
        library.set_favorite(favorite, true).unwrap();
        library.record_play(played, 100).unwrap();
        let order: Vec<i64> = library
            .files_to_analyze(1, 10)
            .unwrap()
            .iter()
            .map(|j| j.track_id)
            .collect();
        assert_eq!(&order[..2], &[favorite, played]);
    }

    #[test]
    fn links_are_listed_apart_and_counted() {
        let library = sample_library();
        let link = add_link(&library, "https://music.example/watch?v=1");
        assert!(
            library
                .files_to_analyze(1, 10)
                .unwrap()
                .iter()
                .all(|j| j.track_id != link)
        );
        let links = library.links_to_analyze(1, 10).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].stamp, None);
        let file = id_of(&library, "/m/b/1.flac");
        library
            .set_features(
                file,
                1,
                FeatureSource::File,
                Some((100, 1)),
                Ok(sound(-9.0)),
                1,
            )
            .unwrap();
        library
            .set_features(
                link,
                1,
                FeatureSource::Stream,
                None,
                Err("blocked".into()),
                1,
            )
            .unwrap();
        assert_eq!(
            library.analysis_progress(1).unwrap(),
            AnalysisProgress {
                files_done: 1,
                files_total: 4,
                links_done: 1,
                links_total: 1,
                failed: 1,
            }
        );
        assert_eq!(library.analysis_progress(2).unwrap().files_done, 0);
    }

    #[test]
    fn stream_decoder_errors_retry_after_a_cooldown() {
        let library = sample_library();
        let link = add_link(&library, "https://music.example/retry");
        let failed = |at| {
            library
                .set_features(
                    link,
                    1,
                    FeatureSource::Stream,
                    None,
                    Err("FFmpeg failed: certificate verify failed".into()),
                    at,
                )
                .unwrap()
        };
        failed(crate::now());
        assert!(library.links_to_analyze(1, 10).unwrap().is_empty());
        failed(crate::now() - 301);
        assert_eq!(library.links_to_analyze(1, 10).unwrap()[0].track_id, link);
        library
            .set_features(
                link,
                1,
                FeatureSource::Stream,
                None,
                Ok(sound(-9.0)),
                crate::now(),
            )
            .unwrap();
        assert!(library.links_to_analyze(1, 10).unwrap().is_empty());
    }

    #[test]
    fn tls_upgrade_retries_only_failed_stream_decodes_once() {
        let mut library = sample_library();
        let retry = add_link(&library, "https://music.example/tls");
        let good = add_link(&library, "https://music.example/good");
        let unavailable = add_link(&library, "https://music.example/gone");
        let file = id_of(&library, "/m/b/1.flac");
        for (id, source, result) in [
            (
                retry,
                FeatureSource::Stream,
                Err("FFmpeg failed: Input/output error".into()),
            ),
            (good, FeatureSource::Stream, Ok(sound(-8.0))),
            (
                unavailable,
                FeatureSource::Stream,
                Err("Video unavailable".into()),
            ),
            (
                file,
                FeatureSource::File,
                Err("FFmpeg failed: invalid file".into()),
            ),
        ] {
            library
                .set_features(id, 1, source, None, result, crate::now())
                .unwrap();
        }
        library
            .conn
            .pragma_update(None, "user_version", 10)
            .unwrap();
        crate::schema::migrate(&mut library.conn).unwrap();
        assert_eq!(
            library
                .links_to_analyze(1, 10)
                .unwrap()
                .iter()
                .map(|j| j.track_id)
                .collect::<Vec<_>>(),
            [retry]
        );
        assert_eq!(library.features(good).unwrap(), Some(sound(-8.0)));
        let rows: i64 = library
            .conn
            .query_row("SELECT COUNT(*) FROM track_features", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 3);
        // Reopening the upgraded database must not delete newly recorded failures again.
        library
            .set_features(
                retry,
                1,
                FeatureSource::Stream,
                None,
                Err("FFmpeg failed: still broken".into()),
                crate::now(),
            )
            .unwrap();
        crate::schema::migrate(&mut library.conn).unwrap();
        assert!(library.links_to_analyze(1, 10).unwrap().is_empty());
    }

    #[test]
    fn candidates_carry_what_the_wave_weighs() {
        let mut library = sample_library();
        let first = id_of(&library, "/m/b/1.flac");
        let second = id_of(&library, "/m/b/2.flac");
        library
            .set_features(first, 1, FeatureSource::File, None, Ok(sound(-8.0)), 1)
            .unwrap();
        library
            .set_features(second, 1, FeatureSource::File, None, Err("x".into()), 1)
            .unwrap();
        library.set_favorite(first, true).unwrap();
        library.record_play(first, 50).unwrap();
        let candidates = library.wave_candidates().unwrap();
        assert_eq!(candidates.len(), 1);
        let c = &candidates[0];
        assert_eq!(c.candidate.id, first);
        assert_eq!(c.candidate.features, sound(-8.0));
        assert!(c.candidate.favorite);
        assert_eq!(c.candidate.plays, 1);
        assert_eq!(c.candidate.last_played, Some(50));
        assert_eq!(c.candidate.year, Some(2020));
        assert_eq!(c.candidate.artists.len(), 1);
        assert!(!c.is_link());
    }

    #[test]
    fn habits_come_from_marks_and_history() {
        let mut library = sample_library();
        let a = id_of(&library, "/m/b/1.flac");
        let b = id_of(&library, "/m/b/2.flac");
        let c = id_of(&library, "/m/a/1.mp3");
        // a → b twice, b → c once but an hour apart (not a sequence).
        for (track, at) in [(a, 1000), (b, 1200), (a, 2000), (b, 2200), (c, 6000)] {
            library.record_play(track, at).unwrap();
        }
        library.mark(c, Mark::Skip { in_wave: true }, 6000).unwrap();
        library.mark(b, Mark::Dislike, 6000).unwrap();
        let habits = library.wave_habits(6000).unwrap();
        assert_eq!(habits.transitions.get(&(a, b)), Some(&2));
        assert_eq!(habits.transitions.get(&(b, a)), Some(&1));
        assert_eq!(habits.transitions.get(&(b, c)), None);
        assert!((habits.skips[&c] - 1.0).abs() < 1e-3);
        // A month later the skip weighs about a third.
        let later = library.wave_habits(6000 + 30 * 24 * 3600).unwrap();
        assert!((later.skips[&c] - 0.368).abs() < 0.01);
        assert!(habits.dislikes.contains(&b));
        // Liking it takes the dislike back.
        library.set_favorite(b, true).unwrap();
        assert!(library.disliked().unwrap().is_empty());
    }
}
