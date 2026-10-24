//! Crossfading between tracks (the pure parts).
//!
//! One mpv can't play two files at once, so a crossfade takes two: the deck playing the
//! current track and a second one that loads the next track, paused and silent, while the
//! current one plays. At the right moment the second starts and becomes the player's deck,
//! and the volumes of both are ramped on one clock, equal-power, so the sum stays as loud.
//! Each song plays whole on one deck, so no two processes ever have to meet on the same
//! sample; when they start a few milliseconds apart, nobody can tell in a crossfade of
//! seconds. Tracks of one album follow each other gapless on one deck, as before.

use std::path::Path;
use std::time::Duration;

use super::queue::Entry;

/// How often the volumes move during a crossfade: small steps, no zipper noise.
pub const RAMP_STEP: Duration = Duration::from_millis(30);

/// Whether two entries are neighbours on one album, which play gapless (live and concept
/// albums must not be blended): the same album name, and the same artist, folder, or both
/// links (an album of links lists various artists).
pub fn same_album(a: &Entry, b: &Entry) -> bool {
    let album = |entry: &Entry| {
        entry
            .album
            .as_deref()
            .map(str::trim)
            .filter(|album| !album.is_empty())
            .map(str::to_lowercase)
    };
    let (Some(first), Some(second)) = (album(a), album(b)) else {
        return false;
    };
    if first != second {
        return false;
    }
    let remote = |entry: &Entry| links::is_remote(&entry.path);
    let folder = |entry: &Entry| Path::new(&entry.path).parent().map(Path::to_path_buf);
    let same_artist = a.artist.is_some() && a.artist == b.artist;
    let same_folder = !remote(a) && !remote(b) && folder(a) == folder(b);
    same_artist || same_folder || (remote(a) && remote(b))
}

/// Where in a track of `duration` seconds a crossfade of `seconds` starts; `None` when the
/// track is too short to blend (it would fade out while still fading in).
pub fn start_at(duration: f64, seconds: f64) -> Option<f64> {
    (seconds > 0.0 && duration.is_finite() && duration >= 2.0 * seconds + 1.0)
        .then_some(duration - seconds)
}

/// The amplitudes of the incoming and the outgoing track at `progress` (0..1) through the
/// fade: equal power, so the two together keep the loudness.
pub fn curves(progress: f64) -> (f64, f64) {
    let angle = progress.clamp(0.0, 1.0) * std::f64::consts::FRAC_PI_2;
    (angle.sin(), angle.cos())
}

/// mpv's `volume` for an amplitude: mpv's volume is cubic (100 is full, 50 is an eighth),
/// on top of the user's own volume.
pub fn deck_volume(user_volume: f64, amplitude: f64) -> f64 {
    let volume = user_volume * amplitude.clamp(0.0, 1.0).cbrt();
    (volume * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, album: Option<&str>, artist: Option<&str>) -> Entry {
        Entry {
            path: path.into(),
            album: album.map(Into::into),
            artist: artist.map(Into::into),
            ..Entry::default()
        }
    }

    #[test]
    fn neighbours_on_an_album_stay_gapless() {
        let a = entry("/m/Album/01.flac", Some("Album"), Some("Band"));
        let b = entry("/m/Album/02.flac", Some("album "), Some("Band"));
        assert!(same_album(&a, &b));
        // A compilation: other artists, one folder.
        let c = entry("/m/Album/03.flac", Some("Album"), Some("Guest"));
        assert!(same_album(&a, &c));
        // The same album name from elsewhere by someone else is another album.
        let d = entry("/m/Other/01.flac", Some("Album"), Some("Someone"));
        assert!(!same_album(&a, &d));
        // Links of one album.
        let e = entry("https://x.example/1", Some("Live"), Some("A"));
        let f = entry("https://x.example/2", Some("Live"), Some("B"));
        assert!(same_album(&e, &f));
        // Without albums, nothing is an album.
        let g = entry("/m/x.flac", None, Some("Band"));
        let h = entry("/m/y.flac", None, Some("Band"));
        assert!(!same_album(&g, &h));
    }

    #[test]
    fn short_tracks_are_not_blended() {
        assert_eq!(start_at(200.0, 6.0), Some(194.0));
        assert_eq!(start_at(12.0, 6.0), None);
        assert_eq!(start_at(200.0, 0.0), None);
        assert_eq!(start_at(f64::NAN, 6.0), None);
    }

    #[test]
    fn the_fade_keeps_the_power() {
        for step in 0..=10 {
            let (incoming, outgoing) = curves(f64::from(step) / 10.0);
            assert!((incoming * incoming + outgoing * outgoing - 1.0).abs() < 1e-9);
        }
        assert_eq!(curves(0.0), (0.0, 1.0));
        let (incoming, outgoing) = curves(1.5);
        assert!((incoming - 1.0).abs() < 1e-12 && outgoing.abs() < 1e-12);
    }

    #[test]
    fn volumes_follow_mpvs_cubic_scale() {
        assert_eq!(deck_volume(80.0, 1.0), 80.0);
        assert_eq!(deck_volume(80.0, 0.0), 0.0);
        // An eighth of the amplitude is half of mpv's volume.
        assert_eq!(deck_volume(100.0, 0.125), 50.0);
    }
}
