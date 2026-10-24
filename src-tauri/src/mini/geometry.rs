//! Where the mini player sits and how big it is: sizes per mode, snapping to the screen's
//! edges, and what is remembered between runs. Pure, so it is unit-tested; the window service
//! (`super`) applies it.

use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// What the mini player shows.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    /// Artwork, title, controls and progress.
    #[default]
    Compact,
    /// One thin line: a small artwork, the title and the main controls.
    Slim,
    /// The playing line of the lyrics, karaoke style.
    Lyrics,
}

/// The size the settings offer; the window can still be dragged to any size in between.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SizePreset {
    Small,
    #[default]
    Medium,
    Large,
}

/// A size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

const fn size(width: f64, height: f64) -> Size {
    Size { width, height }
}

impl Mode {
    /// The medium size; the others scale it.
    fn base(self) -> Size {
        match self {
            Mode::Compact => size(380.0, 124.0),
            Mode::Slim => size(440.0, 52.0),
            Mode::Lyrics => size(560.0, 112.0),
        }
    }

    /// The smallest and largest the window can be dragged to: the layout keeps working
    /// in between (everything scales with the height).
    pub fn limits(self) -> (Size, Size) {
        match self {
            Mode::Compact => (size(280.0, 92.0), size(760.0, 248.0)),
            Mode::Slim => (size(260.0, 40.0), size(900.0, 72.0)),
            Mode::Lyrics => (size(300.0, 72.0), size(1400.0, 260.0)),
        }
    }
}

impl SizePreset {
    fn scale(self) -> f64 {
        match self {
            SizePreset::Small => 0.8,
            SizePreset::Medium => 1.0,
            SizePreset::Large => 1.3,
        }
    }
}

/// The window size a preset gives a mode.
pub fn preset_size(mode: Mode, preset: SizePreset) -> Size {
    let base = mode.base();
    let scale = preset.scale();
    clamp_size(
        mode,
        size((base.width * scale).round(), (base.height * scale).round()),
    )
}

/// A remembered (or odd) size brought within the mode's limits.
pub fn clamp_size(mode: Mode, wanted: Size) -> Size {
    let (min, max) = mode.limits();
    let fit = |value: f64, low: f64, high: f64, fallback: f64| {
        if value.is_finite() {
            value.clamp(low, high)
        } else {
            fallback
        }
    };
    let base = mode.base();
    size(
        fit(wanted.width, min.width, max.width, base.width),
        fit(wanted.height, min.height, max.height, base.height),
    )
}

/// A rectangle in physical pixels (window frames, monitor work areas).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    fn right(self) -> i64 {
        i64::from(self.x) + i64::from(self.width)
    }

    fn bottom(self) -> i64 {
        i64::from(self.y) + i64::from(self.height)
    }
}

/// Where the window goes when one of its edges is within `threshold` of the same edge of the
/// work area (the screen without the menu bar, Dock or taskbar): flush against it. `None`
/// when no edge is that close.
pub fn snap(window: Rect, area: Rect, threshold: i64) -> Option<(i32, i32)> {
    let axis = |start: i64, end: i64, area_start: i64, area_end: i64| -> Option<i64> {
        if (start - area_start).abs() <= threshold {
            Some(area_start)
        } else if (end - area_end).abs() <= threshold {
            Some(area_end - (end - start))
        } else {
            None
        }
    };
    let x = axis(
        i64::from(window.x),
        window.right(),
        i64::from(area.x),
        area.right(),
    );
    let y = axis(
        i64::from(window.y),
        window.bottom(),
        i64::from(area.y),
        area.bottom(),
    );
    let x = x.unwrap_or(i64::from(window.x));
    let y = y.unwrap_or(i64::from(window.y));
    let snapped = (i32::try_from(x).ok()?, i32::try_from(y).ok()?);
    (snapped != (window.x, window.y)).then_some(snapped)
}

/// Where a window that sticks out of the work area goes to be inside it (a larger mode, a
/// larger preset); `None` when it is inside already. A window larger than the area keeps its
/// top-left corner on it.
pub fn fit_inside(window: Rect, area: Rect) -> Option<(i32, i32)> {
    let axis = |start: i64, length: u32, area_start: i64, area_length: u32| -> i64 {
        let last = area_start + i64::from(area_length) - i64::from(length);
        start.min(last).max(area_start)
    };
    let x = axis(
        i64::from(window.x),
        window.width,
        i64::from(area.x),
        area.width,
    );
    let y = axis(
        i64::from(window.y),
        window.height,
        i64::from(area.y),
        area.height,
    );
    let fitted = (i32::try_from(x).ok()?, i32::try_from(y).ok()?);
    (fitted != (window.x, window.y)).then_some(fitted)
}

/// Whether enough of the window lies on one of the screens to grab it: a remembered position
/// on a monitor that has since been unplugged must not hide the mini player.
pub fn reachable(window: Rect, areas: &[Rect]) -> bool {
    let need_x = i64::from(window.width.min(80));
    let need_y = i64::from(window.height.min(32));
    areas.iter().any(|area| {
        let overlap_x = window.right().min(area.right()) - i64::from(window.x.max(area.x));
        let overlap_y = window.bottom().min(area.bottom()) - i64::from(window.y.max(area.y));
        overlap_x >= need_x && overlap_y >= need_y
    })
}

/// The top-left corner that puts a window of `width`×`height` near the bottom right of the
/// area, out of the way of what is in the middle of the screen.
pub fn default_position(width: u32, height: u32, area: Rect) -> (i32, i32) {
    let margin = 24_i64;
    let x = (area.right() - i64::from(width) - margin).max(i64::from(area.x));
    let y = (area.bottom() - i64::from(height) - margin).max(i64::from(area.y));
    (
        i32::try_from(x).unwrap_or(area.x),
        i32::try_from(y).unwrap_or(area.y),
    )
}

/// What the mini player keeps between runs (`<app config>/mini-player.json`), apart from the
/// settings: moving the window shouldn't rewrite the settings and notify every window.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Saved {
    /// It was open when the app quit, so it opens again.
    pub open: bool,
    /// Top-left corner in physical pixels.
    pub position: Option<(i32, i32)>,
    /// The size each mode was last dragged to.
    pub sizes: BTreeMap<Mode, Size>,
}

impl Saved {
    /// Anything unreadable counts as nothing remembered.
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|err| {
                log::warn!("ignoring {}: {err}", path.display());
                Self::default()
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => Self::default(),
            Err(err) => {
                log::warn!("cannot read {}: {err}", path.display());
                Self::default()
            }
        }
    }

    /// Writes through a temporary file, so a crash never leaves half a file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 25,
        width: 1440,
        height: 875,
    };

    fn window(x: i32, y: i32) -> Rect {
        Rect {
            x,
            y,
            width: 380,
            height: 124,
        }
    }

    #[test]
    fn presets_scale_each_mode_within_its_limits() {
        assert_eq!(
            preset_size(Mode::Compact, SizePreset::Medium),
            size(380.0, 124.0)
        );
        assert_eq!(
            preset_size(Mode::Slim, SizePreset::Small),
            size(352.0, 42.0)
        );
        assert_eq!(
            preset_size(Mode::Lyrics, SizePreset::Large),
            size(728.0, 146.0)
        );
        for mode in [Mode::Compact, Mode::Slim, Mode::Lyrics] {
            let (min, max) = mode.limits();
            for preset in [SizePreset::Small, SizePreset::Medium, SizePreset::Large] {
                let size = preset_size(mode, preset);
                assert!(size.width >= min.width && size.width <= max.width);
                assert!(size.height >= min.height && size.height <= max.height);
            }
        }
    }

    #[test]
    fn odd_remembered_sizes_are_brought_back() {
        assert_eq!(
            clamp_size(Mode::Slim, size(5000.0, 10.0)),
            size(900.0, 40.0)
        );
        assert_eq!(
            clamp_size(Mode::Compact, size(f64::NAN, 130.0)),
            size(380.0, 130.0)
        );
    }

    #[test]
    fn edges_close_to_the_screens_snap_to_it() {
        // Near the left and top edges (below the menu bar).
        assert_eq!(snap(window(9, 37), SCREEN, 16), Some((0, 25)));
        // Near the right edge only.
        assert_eq!(
            snap(window(1440 - 380 - 12, 400), SCREEN, 16),
            Some((1060, 400))
        );
        // Near the bottom edge only.
        assert_eq!(
            snap(window(500, 900 - 124 - 5), SCREEN, 16),
            Some((500, 776))
        );
        // In the middle: nothing.
        assert_eq!(snap(window(500, 400), SCREEN, 16), None);
        // Already flush: nothing to do.
        assert_eq!(snap(window(0, 25), SCREEN, 16), None);
        // A little past the edge (dragged half off) comes back too.
        assert_eq!(snap(window(-10, 400), SCREEN, 16), Some((0, 400)));
    }

    #[test]
    fn a_window_grown_past_the_edge_comes_back_inside() {
        let grown = Rect {
            width: 728,
            height: 146,
            ..window(1000, 800)
        };
        assert_eq!(fit_inside(grown, SCREEN), Some((712, 754)));
        assert_eq!(fit_inside(window(100, 100), SCREEN), None);
        let huge = Rect {
            width: 3000,
            height: 2000,
            ..window(100, 100)
        };
        assert_eq!(fit_inside(huge, SCREEN), Some((0, 25)));
    }

    #[test]
    fn a_window_on_an_unplugged_screen_is_unreachable() {
        let second = Rect {
            x: 1440,
            y: 0,
            width: 1920,
            height: 1080,
        };
        assert!(reachable(window(100, 100), &[SCREEN]));
        assert!(reachable(window(2000, 500), &[SCREEN, second]));
        assert!(!reachable(window(2000, 500), &[SCREEN]));
        // Barely a corner on the screen isn't enough to grab it.
        assert!(!reachable(window(1430, 880), &[SCREEN]));
    }

    #[test]
    fn opens_near_the_bottom_right_by_default() {
        assert_eq!(default_position(380, 124, SCREEN), (1036, 752));
        // A window larger than the screen starts at its corner.
        assert_eq!(default_position(2000, 2000, SCREEN), (0, 25));
    }

    #[test]
    fn remembers_between_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mini-player.json");
        assert_eq!(Saved::load(&path), Saved::default());

        let mut saved = Saved {
            open: true,
            position: Some((-1200, 40)),
            ..Saved::default()
        };
        saved.sizes.insert(Mode::Lyrics, size(600.0, 120.0));
        saved.save(&path).unwrap();
        assert_eq!(Saved::load(&path), saved);

        fs::write(&path, "{ broken").unwrap();
        assert_eq!(Saved::load(&path), Saved::default());
    }
}
