use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};

use crate::MediaError;
use crate::palette::{Palette, extract_palette};

/// Encoded image bytes as found in a tag or file.
#[derive(Debug, Clone, PartialEq)]
pub struct Artwork {
    pub bytes: Vec<u8>,
    pub mime: Option<String>,
}

/// Cover file names next to a track, most likely first (matched case-insensitively).
const FOLDER_NAMES: &[&str] = &["cover", "folder", "front", "album", "albumart"];
const FOLDER_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Looks for an album cover image in the track's directory (`cover.jpg`, `Folder.png`, ...).
pub fn folder_artwork(track: &Path) -> Option<Artwork> {
    let dir = track.parent()?;
    let files: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    let lower = |s: Option<&std::ffi::OsStr>| s.and_then(|s| s.to_str()).map(str::to_lowercase);

    FOLDER_NAMES.iter().find_map(|name| {
        files.iter().find_map(|path| {
            let ext = lower(path.extension())?;
            if lower(path.file_stem()).as_deref() != Some(name)
                || !FOLDER_EXTENSIONS.contains(&ext.as_str())
            {
                return None;
            }
            let bytes = fs::read(path).ok()?;
            let mime = image::guess_format(&bytes)
                .ok()
                .map(|f| f.to_mime_type().to_owned());
            Some(Artwork { bytes, mime })
        })
    })
}

/// The MIME type of encoded image bytes, if they are an image.
pub fn image_mime(bytes: &[u8]) -> Option<String> {
    image::guess_format(bytes)
        .ok()
        .map(|format| format.to_mime_type().to_owned())
}

/// Artwork ready for the UI.
#[derive(Debug, Clone)]
pub struct PreparedArtwork {
    /// For display: the original, or a JPEG copy when the original is very large.
    pub image: Artwork,
    /// A small JPEG for the window background, already blurred and saturated (see
    /// [`backdrop_image`]).
    pub thumbnail: Vec<u8>,
    pub palette: Option<Palette>,
}

/// Larger covers are downscaled before they reach the WebView.
const DISPLAY_MAX: u32 = 1200;
/// The backdrop blurs heavily, so a tiny image looks the same and costs far less.
const THUMBNAIL_MAX: u32 = 96;
/// The backdrop's blur, in pixels of the 96 px image: about what `blur(56px)` was on a
/// window-sized copy.
const BACKDROP_BLUR: f32 = 4.0;
const BACKDROP_SATURATION: f32 = 1.4;
/// Enough pixels for stable dominant colors.
const PALETTE_SAMPLE: u32 = 48;

/// Decodes artwork once to produce the display image, backdrop thumbnail and palette.
pub fn prepare_artwork(artwork: Artwork) -> Result<PreparedArtwork, MediaError> {
    let decoded = image::load_from_memory(&artwork.bytes)?;
    let (width, height) = decoded.dimensions();

    let image = if width.max(height) > DISPLAY_MAX {
        let resized = decoded.resize(DISPLAY_MAX, DISPLAY_MAX, FilterType::Triangle);
        Artwork {
            bytes: encode_jpeg(&resized, 88)?,
            mime: Some("image/jpeg".to_owned()),
        }
    } else {
        let mime = artwork.mime.or_else(|| {
            image::guess_format(&artwork.bytes)
                .ok()
                .map(|f| f.to_mime_type().to_owned())
        });
        Artwork {
            bytes: artwork.bytes,
            mime,
        }
    };

    let thumbnail = encode_jpeg(&backdrop_image(&decoded), 80)?;
    let sample = decoded.thumbnail(PALETTE_SAMPLE, PALETTE_SAMPLE).to_rgb8();
    let palette = extract_palette(sample.pixels().map(|p| p.0));

    Ok(PreparedArtwork {
        image,
        thumbnail,
        palette,
    })
}

/// A JPEG at most `max` pixels on its longer side, for album grids and lists.
pub fn cover_thumbnail(bytes: &[u8], max: u32) -> Result<Vec<u8>, MediaError> {
    let decoded = image::load_from_memory(bytes)?;
    let (width, height) = decoded.dimensions();
    let image = if width.max(height) > max {
        decoded.resize(max, max, FilterType::Triangle)
    } else {
        decoded
    };
    encode_jpeg(&image, 85)
}

/// A square JPEG or PNG cover (what every tag format takes) from a thumbnail: its centre, as
/// wide as it is high. YouTube shows album art inside 16:9 thumbnails, with bars at the
/// sides; this cuts them off. A square (within 2 %) JPEG or PNG comes back as it is.
pub fn square_cover(bytes: &[u8]) -> Result<Vec<u8>, MediaError> {
    let decoded = image::load_from_memory(bytes)?;
    let (width, height) = decoded.dimensions();
    let side = width.min(height);
    let square = side > 0 && width.max(height) - side <= side / 50;
    let taggable = matches!(
        image::guess_format(bytes),
        Ok(image::ImageFormat::Jpeg | image::ImageFormat::Png)
    );
    if square && taggable {
        return Ok(bytes.to_vec());
    }
    if square || side == 0 {
        return encode_jpeg(&decoded, 92);
    }
    let cropped = decoded.crop_imm((width - side) / 2, (height - side) / 2, side, side);
    encode_jpeg(&cropped, 92)
}

/// The window background's image: small, blurred and saturated here once, so the window only
/// moves and scales it. A CSS `filter` on a moving layer would be redone on every frame.
fn backdrop_image(image: &DynamicImage) -> DynamicImage {
    let small = image.thumbnail(THUMBNAIL_MAX, THUMBNAIL_MAX).to_rgb8();
    let mut blurred = image::imageops::blur(&small, BACKDROP_BLUR);
    for pixel in blurred.pixels_mut() {
        let [r, g, b] = pixel.0.map(f32::from);
        let luma = 0.299 * r + 0.587 * g + 0.114 * b;
        pixel.0 = [r, g, b].map(|c| {
            // Clamped to a byte's range first, so the cast can't overflow.
            (luma + (c - luma) * BACKDROP_SATURATION)
                .round()
                .clamp(0.0, 255.0) as u8
        });
    }
    DynamicImage::ImageRgb8(blurred)
}

fn encode_jpeg(image: &DynamicImage, quality: u8) -> Result<Vec<u8>, MediaError> {
    let mut out = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut out, quality).encode_image(&image.to_rgb8())?;
    Ok(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::from_fn(width, height, |x, _| {
            // Black bars at the sides of a 16:9 frame, the "cover" in the middle.
            if x < (width - height) / 2 || x >= (width + height) / 2 {
                image::Rgb([0, 0, 0])
            } else {
                image::Rgb([200, 40, 40])
            }
        })
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
        out.into_inner()
    }

    #[test]
    fn the_backdrop_image_comes_blurred() {
        // Hard edges between the bars and the red middle...
        let decoded = image::load_from_memory(&png(160, 90)).unwrap();
        let backdrop = backdrop_image(&decoded).to_rgb8();
        assert!(backdrop.width() <= THUMBNAIL_MAX && backdrop.height() <= THUMBNAIL_MAX);
        // ...become a gradient: no two neighbours far apart.
        let row: Vec<i32> = (0..backdrop.width())
            .map(|x| i32::from(backdrop.get_pixel(x, backdrop.height() / 2).0[0]))
            .collect();
        let steepest = row.windows(2).map(|w| (w[0] - w[1]).abs()).max().unwrap();
        assert!(steepest < 40, "{steepest}");
        // And more saturated: red stands further from green than in the source (200 vs 40).
        let middle = backdrop
            .get_pixel(backdrop.width() / 2, backdrop.height() / 2)
            .0;
        assert!(middle[0] > middle[1] + 160, "{middle:?}");
    }

    #[test]
    fn cuts_wide_thumbnails_to_the_square_cover() {
        let square = square_cover(&png(160, 90)).unwrap();
        let decoded = image::load_from_memory(&square).unwrap();
        assert_eq!(decoded.dimensions(), (90, 90));
        // No bar left at the edges.
        let edge = decoded.to_rgb8().get_pixel(0, 45).0;
        assert!(edge[0] > 150, "{edge:?}");
        let already = png(100, 100);
        assert_eq!(square_cover(&already).unwrap(), already);
        // Other formats become JPEG.
        let mut webp = Cursor::new(Vec::new());
        image::load_from_memory(&already)
            .unwrap()
            .write_to(&mut webp, image::ImageFormat::WebP)
            .unwrap();
        let converted = square_cover(webp.get_ref()).unwrap();
        assert_eq!(
            image::guess_format(&converted).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert!(square_cover(b"not an image").is_err());
    }
    use crate::palette::Rgb;
    use image::{ImageFormat, Rgb as Pixel, RgbImage};

    /// Left 70 % red, right 30 % blue.
    fn two_color_png(width: u32, height: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(width, height, |x, _| {
            if x < width * 7 / 10 {
                Pixel([200, 30, 40])
            } else {
                Pixel([30, 60, 200])
            }
        });
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn cover_thumbnails_are_small_jpegs() {
        let thumb = cover_thumbnail(&two_color_png(1000, 500), 512).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert_eq!(image::guess_format(&thumb).unwrap(), ImageFormat::Jpeg);
        assert_eq!(decoded.dimensions(), (512, 256));
        let small = cover_thumbnail(&two_color_png(100, 100), 512).unwrap();
        assert_eq!(
            image::load_from_memory(&small).unwrap().dimensions(),
            (100, 100)
        );
    }

    #[test]
    fn small_artwork_is_kept_as_is() {
        let png = two_color_png(300, 300);
        let prepared = prepare_artwork(Artwork {
            bytes: png.clone(),
            mime: None,
        })
        .unwrap();
        assert_eq!(prepared.image.bytes, png);
        assert_eq!(prepared.image.mime.as_deref(), Some("image/png"));
    }

    #[test]
    fn large_artwork_is_downscaled_to_jpeg() {
        let prepared = prepare_artwork(Artwork {
            bytes: two_color_png(2400, 1200),
            mime: Some("image/png".into()),
        })
        .unwrap();
        assert_eq!(prepared.image.mime.as_deref(), Some("image/jpeg"));
        let display = image::load_from_memory(&prepared.image.bytes).unwrap();
        assert_eq!(display.dimensions(), (1200, 600));
    }

    #[test]
    fn makes_a_small_thumbnail_and_a_palette() {
        let prepared = prepare_artwork(Artwork {
            bytes: two_color_png(400, 400),
            mime: None,
        })
        .unwrap();
        let thumb = image::load_from_memory(&prepared.thumbnail).unwrap();
        assert!(thumb.width() <= THUMBNAIL_MAX && thumb.height() <= THUMBNAIL_MAX);

        let palette = prepared.palette.unwrap();
        let near = |a: Rgb, b: Rgb| {
            [(a.0, b.0), (a.1, b.1), (a.2, b.2)]
                .iter()
                .all(|(x, y)| x.abs_diff(*y) < 16)
        };
        assert!(near(palette.colors[0], Rgb(200, 30, 40)));
        assert!(palette.colors.iter().any(|&c| near(c, Rgb(30, 60, 200))));
    }

    #[test]
    fn rejects_bytes_that_are_not_an_image() {
        let result = prepare_artwork(Artwork {
            bytes: b"definitely not a picture".to_vec(),
            mime: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn finds_cover_files_case_insensitively_in_preference_order() {
        let tmp = tempfile::tempdir().unwrap();
        let track = tmp.path().join("01 - Song.flac");
        fs::write(&track, b"").unwrap();
        fs::write(tmp.path().join("Folder.PNG"), two_color_png(4, 4)).unwrap();
        fs::write(tmp.path().join("COVER.jpg"), two_color_png(8, 8)).unwrap();
        fs::write(tmp.path().join("cover.txt"), b"not an image").unwrap();

        let found = folder_artwork(&track).unwrap();
        assert_eq!(found.bytes, two_color_png(8, 8));
        assert_eq!(found.mime.as_deref(), Some("image/png"));
    }

    #[test]
    fn no_cover_file_means_none() {
        let tmp = tempfile::tempdir().unwrap();
        let track = tmp.path().join("song.mp3");
        fs::write(&track, b"").unwrap();
        assert_eq!(folder_artwork(&track), None);
    }
}
