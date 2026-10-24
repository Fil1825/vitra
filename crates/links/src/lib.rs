//! Links: what a pasted URL is, what yt-dlp says about it, and how to download it.
//!
//! The pure parts (classification, metadata parsing, file names, progress lines, the download
//! state machine) are tested against fixtures; [`ytdlp`] runs the yt-dlp program itself.

mod classify;
mod download;
mod info;
mod progress;
pub mod release;
mod template;
pub mod ytdlp;

pub use ytdlp::{COOKIE_BROWSERS, YtDlp, music_search_url};

pub use classify::{Link, LinkKind, is_remote, link_title, parse_link};
pub use download::{AudioFormat, DownloadOptions, DownloadStatus, MP3_BITRATES, download_args};
pub use info::{
    LinkAlbum, LinkPlaylist, LinkTrack, Resolved, is_album_link, parse_entry, parse_info,
    place_in_album,
};
pub use progress::{DownloadEvent, parse_progress_line};
pub use template::{
    DEFAULT_ALBUM_TEMPLATE, DEFAULT_TEMPLATE, FileNameFields, file_stem, relative_path,
    sanitize_file_name, unique_stem,
};

#[derive(Debug, thiserror::Error)]
pub enum LinkError {
    #[error("not a web link: {0}")]
    InvalidUrl(String),
    #[error("yt-dlp was not found")]
    YtDlpMissing,
    /// yt-dlp failed; the message is its last error line.
    #[error("{0}")]
    Failed(String),
    /// The site wants proof of age: only a signed-in browser's cookies get through.
    #[error("the video is age-restricted")]
    AgeRestricted,
    /// The site wants a signed-in user (YouTube's "confirm you're not a bot", members-only).
    #[error("the site asks to sign in")]
    SignInRequired,
    /// yt-dlp couldn't read the browser's cookies (no such browser, or macOS keeps Safari's
    /// from the app without Full Disk Access).
    #[error("cannot read the browser's cookies: {0}")]
    Cookies(String),
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
