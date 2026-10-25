//! Error type returned by Tauri commands.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Tauri(#[from] tauri::Error),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Player(String),
    #[error("{0}")]
    Lyrics(String),
    #[error(transparent)]
    Library(#[from] library::Error),
    #[error(transparent)]
    Media(#[from] media::MediaError),
    #[error(transparent)]
    Link(#[from] links::LinkError),
    #[error(transparent)]
    Theme(#[from] themes::Error),
    /// A playlist import that couldn't be read (the message says what to do instead).
    #[error("{0}")]
    Import(String),
    /// A background task that didn't finish (it panicked or was cancelled).
    #[error("{0}")]
    Task(String),
}

/// Commands reject with the error message; the frontend only needs something readable.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
