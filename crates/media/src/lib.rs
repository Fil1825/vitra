//! Everything Vitra knows about a file before mpv plays it: tags and audio properties (lofty),
//! artwork (embedded or next to the file) and the color palette the UI is tinted with.

mod artwork;
mod lyrics;
mod metadata;
mod palette;
mod tags;

pub use artwork::{Artwork, PreparedArtwork, cover_thumbnail, folder_artwork, prepare_artwork};
pub use lyrics::{TagLyrics, read_lyrics};
pub use metadata::{TrackInfo, TrackMetadata, read_track};
pub use palette::{Palette, Rgb, extract_palette};
pub use tags::{CoverChange, TagValues, write_tags};

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("cannot read tags: {0}")]
    Tags(#[from] lofty::error::FileParseError),
    #[error("cannot write tags: {0}")]
    Write(#[from] lofty::error::FileEncodingError),
    #[error("unsupported file or image format")]
    Unsupported,
    #[error("cannot decode image: {0}")]
    Image(#[from] image::ImageError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
