//! Looking lyrics up on LRCLIB. Requests go through curl (part of macOS and Windows 10+, and
//! of every desktop Linux), like the yt-dlp updates, so the app carries no HTTP client.

use std::process::Stdio;
use std::time::Duration;

use lyrics::lrclib::{self, Query, Record};
use tokio::process::Command;

use crate::error::AppError;

/// LRCLIB asks clients to name themselves.
const USER_AGENT: &str = concat!(
    "Vitra v",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/Fil1825/vitra)"
);
const TIMEOUT: Duration = Duration::from_secs(20);

/// What a lookup came back with.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Found(String),
    /// LRCLIB knows the track has no words.
    Instrumental,
    NotFound,
}

/// Fetches `url`; returns the HTTP status and the body.
async fn fetch(url: &str) -> Result<(u16, String), AppError> {
    let mut curl = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    curl.args(["-sS", "-L", "--max-time", "15", "-A", USER_AGENT])
        // The status code on a line of its own after the body.
        .args(["-w", "\n%{http_code}"])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        curl.creation_flags(CREATE_NO_WINDOW);
    }
    let child = curl
        .spawn()
        .map_err(|err| AppError::Lyrics(format!("cannot run curl: {err}")))?;
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| AppError::Lyrics("LRCLIB did not answer in time".into()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Lyrics(format!(
            "cannot reach LRCLIB: {}",
            stderr.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (body, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    let status = status.trim().parse().unwrap_or(0);
    Ok((status, body.to_owned()))
}

fn answer(record: &Record) -> Answer {
    match record.text() {
        Some(text) => Answer::Found(text.to_owned()),
        None if record.instrumental => Answer::Instrumental,
        None => Answer::NotFound,
    }
}

/// Asks LRCLIB for a track's lyrics: the exact lookup first, then a search.
pub async fn look_up(query: &Query) -> Result<Answer, AppError> {
    if let Some(url) = query.get_url() {
        let (status, body) = fetch(&url).await?;
        match status {
            200 => {
                if let Some(record) = lrclib::parse_record(&body) {
                    let found = answer(&record);
                    if found != Answer::NotFound {
                        return Ok(found);
                    }
                }
            }
            // Not found, or a signature it rejects (a missing album): search instead.
            400 | 404 => {}
            status => return Err(AppError::Lyrics(format!("LRCLIB answered {status}"))),
        }
    }
    let Some(url) = query.search_url() else {
        return Ok(Answer::NotFound);
    };
    let (status, body) = fetch(&url).await?;
    if status != 200 {
        return Err(AppError::Lyrics(format!("LRCLIB answered {status}")));
    }
    let records = lrclib::parse_search(&body);
    Ok(query.best(&records).map_or(Answer::NotFound, answer))
}
