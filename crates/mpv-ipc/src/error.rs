use std::process::ExitStatus;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// mpv answered with an error, e.g. "property unavailable".
    #[error("mpv: {0}")]
    Mpv(String),
    #[error("disconnected from mpv")]
    Disconnected,
    #[error("mpv exited before its IPC server came up ({0})")]
    Exited(ExitStatus),
    #[error("timed out waiting for mpv")]
    Timeout,
}
