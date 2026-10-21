use std::collections::HashMap;
use std::fmt;

use serde::{Serialize, Serializer};

/// An sRGB color; serializes as `#RRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// HSL saturation and lightness, both 0–1.
    fn saturation_lightness(self) -> (f64, f64) {
        let [r, g, b] = [self.0, self.1, self.2].map(|c| f64::from(c) / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let lightness = (max + min) / 2.0;
        let delta = max - min;
        let saturation = if delta == 0.0 {
            0.0
        } else {
            delta / (1.0 - (2.0 * lightness - 1.0).abs())
        };
        (saturation.clamp(0.0, 1.0), lightness)
    }

    fn distance(self, other: Rgb) -> f64 {
        let d = |a: u8, b: u8| f64::from(a) - f64::from(b);
        (d(self.0, other.0).powi(2) + d(self.1, other.1).powi(2) + d(self.2, other.2).powi(2))
            .sqrt()
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// Dominant colors of an image, most prominent first, plus the one best suited as an accent.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Palette {
    pub colors: Vec<Rgb>,
    pub accent: Rgb,
}

/// At most this many colors are reported.
const MAX_COLORS: usize = 5;
/// Clusters closer than this (Euclidean RGB distance) are the same color to the eye.
const MERGE_DISTANCE: f64 = 56.0;

#[derive(Default)]
struct Cluster {
    count: u64,
    sum: [u64; 3],
}

impl Cluster {
    fn mean(&self) -> Rgb {
        let channel = |i: usize| (self.sum[i] / self.count.max(1)) as u8;
        Rgb(channel(0), channel(1), channel(2))
    }

    fn absorb(&mut self, other: &Cluster) {
        self.count += other.count;
        for i in 0..3 {
            self.sum[i] += other.sum[i];
        }
    }
}

/// Finds the dominant colors of an image given as RGB pixels (a small thumbnail is plenty).
///
/// Pixels are bucketed by their top 5 bits per channel, then buckets are merged greedily,
/// largest first, into clusters of visually similar colors. Returns `None` for no pixels.
pub fn extract_palette(pixels: impl IntoIterator<Item = [u8; 3]>) -> Option<Palette> {
    let mut buckets: HashMap<[u8; 3], Cluster> = HashMap::new();
    let mut total = 0u64;
    for [r, g, b] in pixels {
        let bucket = buckets.entry([r >> 3, g >> 3, b >> 3]).or_default();
        bucket.count += 1;
        bucket.sum[0] += u64::from(r);
        bucket.sum[1] += u64::from(g);
        bucket.sum[2] += u64::from(b);
        total += 1;
    }
    if total == 0 {
        return None;
    }

    let mut buckets: Vec<Cluster> = buckets.into_values().collect();
    buckets.sort_by_key(|c| std::cmp::Reverse(c.count));

    let mut clusters: Vec<Cluster> = Vec::new();
    for bucket in &buckets {
        let color = bucket.mean();
        match clusters
            .iter_mut()
            .find(|c| c.mean().distance(color) < MERGE_DISTANCE)
        {
            Some(cluster) => cluster.absorb(bucket),
            None => {
                let mut cluster = Cluster::default();
                cluster.absorb(bucket);
                clusters.push(cluster);
            }
        }
    }
    clusters.sort_by_key(|c| std::cmp::Reverse(c.count));
    clusters.truncate(MAX_COLORS);

    let colors: Vec<Rgb> = clusters.iter().map(Cluster::mean).collect();
    let accent = clusters
        .iter()
        .map(|c| {
            (
                c.mean(),
                accent_score(c.mean(), c.count as f64 / total as f64),
            )
        })
        .filter(|(_, score)| *score > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(colors[0], |(color, _)| color);

    Some(Palette { colors, accent })
}

/// Vivid, mid-lightness colors make good accents; a little area is enough to qualify.
fn accent_score(color: Rgb, share: f64) -> f64 {
    let (saturation, lightness) = color.saturation_lightness();
    if !(0.15..=0.85).contains(&lightness) || saturation < 0.2 {
        return 0.0;
    }
    saturation * share.sqrt() * (1.0 - (lightness - 0.5).abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(parts: &[(Rgb, usize)]) -> Vec<[u8; 3]> {
        parts
            .iter()
            .flat_map(|&(Rgb(r, g, b), n)| std::iter::repeat_n([r, g, b], n))
            .collect()
    }

    fn close(a: Rgb, b: Rgb) -> bool {
        a.distance(b) < 12.0
    }

    #[test]
    fn finds_dominant_colors_in_order() {
        let red = Rgb(200, 30, 40);
        let blue = Rgb(30, 60, 200);
        let palette = extract_palette(image(&[(blue, 400), (red, 600)])).unwrap();
        assert_eq!(palette.colors.len(), 2);
        assert!(close(palette.colors[0], red));
        assert!(close(palette.colors[1], blue));
    }

    #[test]
    fn merges_shades_of_the_same_color() {
        let palette = extract_palette(image(&[
            (Rgb(200, 30, 40), 300),
            (Rgb(210, 36, 44), 300),
            (Rgb(192, 26, 36), 300),
        ]))
        .unwrap();
        assert_eq!(palette.colors.len(), 1);
    }

    #[test]
    fn picks_a_vivid_accent_over_a_dull_majority() {
        let gray = Rgb(90, 92, 96);
        let orange = Rgb(240, 130, 30);
        let palette = extract_palette(image(&[(gray, 900), (orange, 100)])).unwrap();
        assert!(close(palette.colors[0], gray));
        assert!(close(palette.accent, orange));
    }

    #[test]
    fn falls_back_to_the_main_color_without_a_vivid_one() {
        let palette =
            extract_palette(image(&[(Rgb(5, 5, 5), 100), (Rgb(250, 250, 250), 50)])).unwrap();
        assert!(close(palette.accent, Rgb(5, 5, 5)));
    }

    #[test]
    fn reports_at_most_five_colors() {
        let colors: Vec<(Rgb, usize)> = (0..8)
            .map(|i| {
                (
                    Rgb(i * 32, 255 - i * 32, (i % 2) * 255),
                    10 + usize::from(i),
                )
            })
            .collect();
        assert_eq!(extract_palette(image(&colors)).unwrap().colors.len(), 5);
    }

    #[test]
    fn nothing_to_extract_from_no_pixels() {
        assert_eq!(extract_palette(Vec::new()), None);
    }

    #[test]
    fn serializes_as_hex() {
        assert_eq!(
            serde_json::to_string(&Rgb(63, 169, 245)).unwrap(),
            "\"#3FA9F5\""
        );
    }
}
