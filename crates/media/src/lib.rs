//! Everything Vitra knows about a file before mpv plays it: tags and audio properties (lofty),
//! artwork (embedded or next to the file) and the color palette the UI is tinted with.

mod artwork;
mod metadata;
mod palette;

pub use artwork::{Artwork, PreparedArtwork, folder_artwork, prepare_artwork};
pub use metadata::{TrackInfo, TrackMetadata, read_track};
pub use palette::{Palette, Rgb, extract_palette};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("cannot read tags: {0}")]
    Tags(#[from] lofty::error::FileParseError),
    #[error("cannot decode image: {0}")]
    Image(#[from] image::ImageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
