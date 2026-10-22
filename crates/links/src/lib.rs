//! Links: what a pasted URL is, what yt-dlp says about it, and how to download it.
//!
//! The pure parts (classification, metadata parsing, file names, progress lines, the download
//! state machine) are tested against fixtures; [`ytdlp`] runs the yt-dlp program itself.

mod classify;
mod download;
mod info;
mod progress;
mod template;
pub mod ytdlp;

pub use classify::{Link, LinkKind, is_remote, parse_link};
pub use download::{AudioFormat, DownloadOptions, DownloadStatus, MP3_BITRATES, download_args};
pub use info::{LinkPlaylist, LinkTrack, Resolved, parse_entry, parse_info};
pub use progress::{DownloadEvent, parse_progress_line};
pub use template::{DEFAULT_TEMPLATE, FileNameFields, file_stem, sanitize_file_name};

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("not a web link: {0}")]
    InvalidUrl(String),
    #[error("yt-dlp was not found")]
    YtDlpMissing,
    /// yt-dlp failed; the message is its last error line.
    #[error("{0}")]
    Failed(String),
    #[error("yt-dlp did not answer in time")]
    TimedOut,
    #[error("cancelled")]
    Cancelled,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unexpected yt-dlp output: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T, E = LinkError> = std::result::Result<T, E>;
