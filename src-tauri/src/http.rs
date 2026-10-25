//! Fetching web pages with curl (part of macOS, Windows 10+ and every desktop Linux), like the
//! lyrics lookups and yt-dlp updates, so the app carries no HTTP client of its own.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::error::AppError;

/// Music services serve their full pages to browsers; a request that names itself as curl
/// may get a stripped one.
pub const BROWSER: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
     (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
const TIMEOUT: Duration = Duration::from_secs(25);

/// What a fetch came back with.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub body: String,
    /// Where redirects ended.
    pub url: String,
}

fn curl() -> Command {
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// GETs `url` (following redirects) with extra `headers` ("Name: value") and, if given, the
/// cookies in a Netscape cookie file.
pub async fn get(
    url: &str,
    headers: &[String],
    cookies: Option<&Path>,
) -> Result<Response, AppError> {
    let mut command = curl();
    command
        .args([
            "-sS",
            "-L",
            "--compressed",
            "--max-time",
            "20",
            "-A",
            BROWSER,
        ])
        .args(["-H", "Accept-Language: en-US,en;q=0.9,ru;q=0.8"]);
    for header in headers {
        command.arg("-H").arg(header);
    }
    if let Some(file) = cookies {
        command.arg("-b").arg(file);
    }
    // The final URL and the status on lines of their own after the body.
    command
        .args(["-w", "\n%{url_effective}\n%{http_code}"])
        .arg("--")
        .arg(url);
    let child = command
        .spawn()
        .map_err(|err| AppError::Import(format!("cannot run curl: {err}")))?;
    let output = tokio::time::timeout(TIMEOUT, child.wait_with_output())
        .await
        .map_err(|_| AppError::Import("the site did not answer in time".into()))??;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Import(format!(
            "cannot reach the site: {}",
            stderr.trim()
        )));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (rest, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    let (body, final_url) = rest.rsplit_once('\n').unwrap_or(("", rest));
    Ok(Response {
        status: status.trim().parse().unwrap_or(0),
        body: body.to_owned(),
        url: final_url.trim().to_owned(),
    })
}

/// Where a short link (`spotify.link/…`, `deezer.page.link/…`) leads.
pub async fn follow(url: &str) -> Result<String, AppError> {
    let response = get(url, &[], None).await?;
    Ok(response.url)
}
