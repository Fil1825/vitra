//! Loudness Vitra measured itself (EBU R128 integrated loudness and the peak), for volume
//! normalization of tracks without ReplayGain tags. Keyed by path, so it covers files outside
//! the library and links too; a file's size and modification time tell when a measurement
//! no longer applies.

use rusqlite::{OptionalExtension, params};

use crate::{Library, Result};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loudness {
    /// Integrated loudness, LUFS.
    pub integrated: f64,
    /// The highest (true) peak, dBFS, when measured.
    pub peak: Option<f64>,
}

/// A file as it was when measured: size and modification time (seconds). Links have none.
pub type FileStamp = (i64, i64);

impl Library {
    /// What was measured for `path`, unless the file changed since.
    pub fn loudness(&self, path: &str, stamp: Option<FileStamp>) -> Result<Option<Loudness>> {
        let row = self
            .conn
            .query_row(
                "SELECT integrated, peak, file_size, file_mtime FROM loudness WHERE path = ?1",
                [path],
                |row| {
                    Ok((
                        Loudness {
                            integrated: row.get(0)?,
                            peak: row.get(1)?,
                        },
                        row.get::<_, Option<i64>>(2)?,
                        row.get::<_, Option<i64>>(3)?,
                    ))
                },
            )
            .optional()?;
        Ok(row.and_then(|(loudness, size, mtime)| {
            let current = match (size, mtime) {
                (Some(size), Some(mtime)) => Some((size, mtime)),
                _ => None,
            };
            (current == stamp).then_some(loudness)
        }))
    }

    pub fn set_loudness(
        &self,
        path: &str,
        stamp: Option<FileStamp>,
        loudness: Loudness,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO loudness (path, integrated, peak, file_size, file_mtime)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                path,
                loudness.integrated,
                loudness.peak,
                stamp.map(|(size, _)| size),
                stamp.map(|(_, mtime)| mtime),
            ],
        )?;
        Ok(())
    }

    /// The loudness of the album the track at `path` belongs to, from its measured tracks:
    /// their energy averaged over time (as if the album played through), and the highest
    /// peak. `None` for a track without an album or an album nothing was measured of.
    pub fn album_loudness(&self, path: &str) -> Result<Option<Loudness>> {
        let mut statement = self.conn.prepare_cached(
            "SELECT l.integrated, l.peak, s.duration
             FROM tracks t
             JOIN tracks s ON s.album_id = t.album_id
             JOIN loudness l ON l.path = s.path
             WHERE t.path = ?1 AND t.album_id IS NOT NULL",
        )?;
        let rows = statement
            .query_map([path], |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, Option<f64>>(1)?,
                    row.get::<_, Option<f64>>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(combine(&rows))
    }
}

/// Integrated loudness values weighted by duration (1 when unknown), in the energy domain.
fn combine(parts: &[(f64, Option<f64>, Option<f64>)]) -> Option<Loudness> {
    let mut energy = 0.0;
    let mut weight = 0.0;
    let mut peak: Option<f64> = None;
    for &(integrated, part_peak, duration) in parts {
        let w = duration
            .filter(|d| d.is_finite() && *d > 0.0)
            .unwrap_or(1.0);
        energy += w * 10f64.powf(integrated / 10.0);
        weight += w;
        if let Some(p) = part_peak {
            peak = Some(peak.map_or(p, |known| known.max(p)));
        }
    }
    (weight > 0.0).then(|| Loudness {
        integrated: 10.0 * (energy / weight).log10(),
        peak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurements_are_kept_until_the_file_changes() {
        let library = Library::open_in_memory().unwrap();
        let loud = Loudness {
            integrated: -9.5,
            peak: Some(0.3),
        };
        assert_eq!(library.loudness("/a.flac", Some((10, 20))).unwrap(), None);
        library
            .set_loudness("/a.flac", Some((10, 20)), loud)
            .unwrap();
        assert_eq!(
            library.loudness("/a.flac", Some((10, 20))).unwrap(),
            Some(loud)
        );
        assert_eq!(library.loudness("/a.flac", Some((10, 21))).unwrap(), None);
        // Links have no file to compare.
        library
            .set_loudness("https://example.com/x", None, loud)
            .unwrap();
        assert_eq!(
            library.loudness("https://example.com/x", None).unwrap(),
            Some(loud)
        );
    }

    #[test]
    fn an_album_is_as_loud_as_its_tracks_together() {
        // Two equal halves: the same loudness.
        let same = combine(&[
            (-10.0, Some(-1.0), Some(100.0)),
            (-10.0, Some(0.5), Some(100.0)),
        ])
        .unwrap();
        assert!((same.integrated + 10.0).abs() < 1e-9);
        assert_eq!(same.peak, Some(0.5));
        // A quiet track pulls the album down far less than an average of the numbers would.
        let mixed = combine(&[(-8.0, None, Some(200.0)), (-20.0, None, Some(200.0))]).unwrap();
        assert!(mixed.integrated > -11.0 && mixed.integrated < -10.5);
        assert_eq!(combine(&[]), None);
    }
}
