//! Running yt-dlp: resolving links, filling in playlist entries, downloading, updating.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};

use crate::download::{DownloadOptions, download_args};
use crate::info::{LinkTrack, Resolved, parse_entry, parse_info};
use crate::progress::{DownloadEvent, parse_progress_line};
use crate::{LinkError, Result};

/// Resolving a link means one page load or a playlist listing.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(90);
/// `yt-dlp -U` downloads a new binary.
const UPDATE_TIMEOUT: Duration = Duration::from_secs(300);
/// The first run of a freshly installed yt-dlp on macOS waits for the system to check its
/// libraries (~25 s on an M-series Mac).
const VERSION_TIMEOUT: Duration = Duration::from_secs(120);
/// stderr lines kept for the error message of a failed download.
const ERROR_LINES: usize = 20;
const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Browsers yt-dlp can take cookies from (`--cookies-from-browser`).
pub const COOKIE_BROWSERS: &[&str] = &[
    "safari", "chrome", "firefox", "edge", "brave", "chromium", "opera", "vivaldi",
];

/// A yt-dlp installation and the helpers it uses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct YtDlp {
    pub binary: PathBuf,
    /// For converting audio and embedding covers.
    pub ffmpeg: Option<PathBuf>,
    /// The JavaScript runtime YouTube needs (since 2025.11).
    pub deno: Option<PathBuf>,
    /// Signs requests in as the user of this browser (one of [`COOKIE_BROWSERS`]), for
    /// age-restricted videos.
    pub cookies_from_browser: Option<String>,
}

/// What a failed run means, from its stderr: errors with a remedy get their own kind. mpv's
/// yt-dlp hook logs the same text, so the player reads its errors with this too.
pub fn failure(stderr: &str) -> LinkError {
    let message = error_message(stderr);
    let lower = message.to_lowercase();
    let cookie_store = lower.contains("cookie");
    if cookie_store
        && (lower.contains("operation not permitted")
            || lower.contains("permission denied")
            || lower.contains("could not find")
            || lower.contains("could not copy")
            || lower.contains("failed to decrypt")
            || lower.contains("unsupported browser"))
    {
        LinkError::Cookies(message)
    } else if lower.contains("confirm your age")
        || lower.contains("age-restricted")
        || lower.contains("age restricted")
        || lower.contains("inappropriate for some users")
    {
        LinkError::AgeRestricted
    } else if lower.contains("not a bot")
        || lower.contains("sign in to confirm")
        || lower.contains("members-only")
        || lower.contains("join this channel")
    {
        LinkError::SignInRequired
    } else {
        LinkError::Failed(message)
    }
}

/// The last `ERROR:` line yt-dlp printed, or its last line at all.
fn error_message(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("ERROR: "))
        .or(lines.last().copied())
        .unwrap_or("yt-dlp failed")
        .to_owned()
}

/// A fresh directory under `parent` for one run's thumbnails.
fn scratch_dir(parent: &Path) -> std::io::Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = parent.join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The image yt-dlp wrote for `id` (any extension), or any image when `id` is unknown.
fn thumbnail_in(dir: &Path, id: Option<&str>) -> Option<Vec<u8>> {
    let entries = std::fs::read_dir(dir).ok()?;
    entries
        .flatten()
        .map(|entry| entry.path())
        .find_map(|path| {
            let ext = path.extension()?.to_str()?.to_ascii_lowercase();
            let stem = path.file_stem()?.to_str()?;
            let matches = id.is_none_or(|id| stem == id);
            (matches && IMAGE_EXTENSIONS.contains(&ext.as_str()))
                .then(|| std::fs::read(&path).ok())
                .flatten()
        })
}

/// Sends every line of `reader` to `tx`, tagged with `is_stderr`.
async fn forward_lines(
    reader: impl AsyncRead + Unpin,
    is_stderr: bool,
    tx: mpsc::Sender<(bool, String)>,
) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if tx.send((is_stderr, line)).await.is_err() {
            break;
        }
    }
}

/// `-o` templates that put every thumbnail into `dir`, named by id: the entries' and the
/// playlist's own (yt-dlp writes that one by `pl_thumbnail:`, else into its working folder).
fn thumbnail_templates(dir: &Path) -> [OsString; 4] {
    let template = |kind: &str| {
        let mut template = OsString::from(kind);
        template.push(dir.join("%(id)s.%(ext)s"));
        template
    };
    [
        "-o".into(),
        template("thumbnail:"),
        "-o".into(),
        template("pl_thumbnail:"),
    ]
}

impl YtDlp {
    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        // The user's own yt-dlp config (output templates, formats) must not change what
        // Vitra gets back.
        command.arg("--ignore-config").arg("--no-warnings");
        if let Some(browser) = self
            .cookies_from_browser
            .as_deref()
            .filter(|b| COOKIE_BROWSERS.contains(b))
        {
            command.arg("--cookies-from-browser").arg(browser);
        }
        if let Some(deno) = &self.deno {
            let mut runtime = OsString::from("deno:");
            runtime.push(deno);
            command.arg("--js-runtimes").arg(runtime);
        }
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        {
            // A console program: don't flash a console window.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command
    }

    fn spawn_error(err: std::io::Error) -> LinkError {
        if err.kind() == std::io::ErrorKind::NotFound {
            LinkError::YtDlpMissing
        } else {
            LinkError::Io(err)
        }
    }

    async fn output(mut command: Command, timeout: Duration) -> Result<Output> {
        let child = command.spawn().map_err(Self::spawn_error)?;
        let output = tokio::time::timeout(timeout, child.wait_with_output())
            .await
            .map_err(|_| LinkError::TimedOut)??;
        if output.status.success() {
            Ok(output)
        } else {
            Err(failure(&String::from_utf8_lossy(&output.stderr)))
        }
    }

    pub async fn version(&self) -> Result<String> {
        let mut command = self.command();
        command.arg("--version");
        let output = Self::output(command, VERSION_TIMEOUT).await?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    /// What `url` is: one track (with its thumbnail) or a playlist (entries listed without
    /// opening each one, plus the playlist's own thumbnail when the site has one).
    /// `scratch` is a directory for temporary files.
    pub async fn resolve(&self, url: &str, scratch: &Path) -> Result<(Resolved, Option<Vec<u8>>)> {
        let dir = scratch_dir(scratch)?;
        let mut command = self.command();
        command
            .args(["-J", "--flat-playlist", "--no-playlist"])
            // Print the JSON *and* write the thumbnail, in one run.
            .args(["--no-simulate", "--skip-download", "--write-thumbnail"])
            .args(thumbnail_templates(&dir))
            // Whatever else it writes lands in the scratch folder, not the app's.
            .current_dir(&dir)
            .arg("--")
            .arg(url);
        let result = Self::output(command, RESOLVE_TIMEOUT)
            .await
            .and_then(|output| {
                let resolved = parse_info(&String::from_utf8_lossy(&output.stdout), url)?;
                Ok((resolved, thumbnail_in(&dir, None)))
            });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    /// Complete metadata and the thumbnail of every entry of `url`, one at a time as yt-dlp
    /// gets them (a playlist listing only has titles). Entries that fail are skipped.
    pub async fn enrich(
        &self,
        url: &str,
        scratch: &Path,
        mut on_track: impl FnMut(LinkTrack, Option<Vec<u8>>),
    ) -> Result<usize> {
        let dir = scratch_dir(scratch)?;
        let mut command = self.command();
        command
            .args([
                "-j",
                "--no-simulate",
                "--skip-download",
                "--write-thumbnail",
            ])
            .args(["--ignore-errors", "--yes-playlist"])
            .args(thumbnail_templates(&dir))
            .current_dir(&dir)
            .arg("--")
            .arg(url);
        let mut child = command.spawn().map_err(Self::spawn_error)?;
        let mut stderr = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut text = String::new();
            if let Some(stderr) = stderr.as_mut() {
                let _ = stderr.read_to_string(&mut text).await;
            }
            text
        });

        let mut count = 0;
        // Thumbnails may land after their JSON line; those are sent at the end.
        let mut waiting: Vec<LinkTrack> = Vec::new();
        if let Some(stdout) = child.stdout.take() {
            let mut lines = BufReader::new(stdout).lines();
            while let Some(line) = lines.next_line().await? {
                let Ok(Some(track)) = parse_entry(&line) else {
                    continue;
                };
                count += 1;
                match thumbnail_in(&dir, track.id.as_deref()) {
                    Some(image) => on_track(track, Some(image)),
                    None => waiting.push(track),
                }
            }
        }
        let status = child.wait().await?;
        for track in waiting {
            let image = thumbnail_in(&dir, track.id.as_deref());
            on_track(track, image);
        }
        let _ = std::fs::remove_dir_all(&dir);
        let stderr = stderr_task.await.unwrap_or_default();
        if count == 0 && !status.success() {
            return Err(failure(&stderr));
        }
        Ok(count)
    }

    /// Downloads `url` as described by `options`, reporting progress to `on_event`. Setting
    /// `cancel` to `true` kills yt-dlp; partial files stay in `options.temp_dir` for the
    /// caller to delete. Returns the finished file.
    pub async fn download(
        &self,
        url: &str,
        options: &DownloadOptions,
        mut cancel: watch::Receiver<bool>,
        mut on_event: impl FnMut(&DownloadEvent),
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(&options.dir)?;
        std::fs::create_dir_all(&options.temp_dir)?;
        let mut command = self.command();
        command.args(download_args(url, options));
        let mut child = command.spawn().map_err(Self::spawn_error)?;
        // Download progress goes to stdout but post-processing progress to stderr, so both
        // are read, through one channel that closes when yt-dlp closes both.
        let (lines_tx, mut lines) = mpsc::channel::<(bool, String)>(64);
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(forward_lines(stdout, false, lines_tx.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(forward_lines(stderr, true, lines_tx.clone()));
        }
        drop(lines_tx);

        let mut file = None;
        let mut errors: VecDeque<String> = VecDeque::new();
        loop {
            tokio::select! {
                line = lines.recv() => match line {
                    Some((is_stderr, line)) => {
                        if let Some(event) = parse_progress_line(&line) {
                            if let DownloadEvent::File(path) = &event {
                                file = Some(path.clone());
                            }
                            on_event(&event);
                        } else if is_stderr {
                            if errors.len() == ERROR_LINES {
                                errors.pop_front();
                            }
                            errors.push_back(line);
                        }
                    }
                    None => break,
                },
                changed = cancel.changed() => {
                    if changed.is_err() || *cancel.borrow() {
                        let _ = child.kill().await;
                        return Err(LinkError::Cancelled);
                    }
                }
            }
        }
        let status = child.wait().await?;
        match file {
            Some(path) if status.success() => Ok(path),
            _ => Err(failure(&errors.into_iter().collect::<Vec<_>>().join("\n"))),
        }
    }

    /// Updates this yt-dlp binary in place (`yt-dlp -U`); returns what it reported.
    pub async fn update(&self) -> Result<String> {
        let mut command = self.command();
        command.arg("-U");
        let output = Self::output(command, UPDATE_TIMEOUT).await?;
        let report = String::from_utf8_lossy(&output.stdout);
        Ok(report
            .lines()
            .map(str::trim)
            .rfind(|line| !line.is_empty())
            .unwrap_or("")
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_prefer_error_lines() {
        assert_eq!(
            error_message("WARNING: slow\nERROR: [youtube] x: Video unavailable\n"),
            "[youtube] x: Video unavailable"
        );
        assert_eq!(error_message("something odd\n\n"), "something odd");
        assert_eq!(error_message(""), "yt-dlp failed");
    }

    #[test]
    fn errors_with_a_remedy_get_their_own_kind() {
        let age = "ERROR: [youtube] abc: Sign in to confirm your age. This video may be \
                   inappropriate for some users. Use --cookies-from-browser or --cookies";
        assert!(matches!(failure(age), LinkError::AgeRestricted));
        let bot = "ERROR: [youtube] abc: Sign in to confirm you’re not a bot.";
        assert!(matches!(failure(bot), LinkError::SignInRequired));
        let safari = "ERROR: [Errno 1] Operation not permitted: \
            '/Users/me/Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies'";
        assert!(matches!(failure(safari), LinkError::Cookies(_)));
        let chrome = "ERROR: could not find chrome cookies database in \"/home/me/.config\"";
        assert!(matches!(failure(chrome), LinkError::Cookies(_)));
        assert!(matches!(
            failure("ERROR: [youtube] abc: Video unavailable"),
            LinkError::Failed(message) if message == "[youtube] abc: Video unavailable"
        ));
    }

    #[test]
    fn finds_thumbnails_by_id() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("abc.webp"), b"webp").unwrap();
        std::fs::write(dir.path().join("abc.json"), b"{}").unwrap();
        std::fs::write(dir.path().join("other.jpg"), b"jpg").unwrap();
        assert_eq!(
            thumbnail_in(dir.path(), Some("abc")),
            Some(b"webp".to_vec())
        );
        assert_eq!(thumbnail_in(dir.path(), Some("missing")), None);
        assert!(thumbnail_in(dir.path(), None).is_some());
    }

    #[tokio::test]
    async fn a_missing_binary_is_reported_as_such() {
        let ytdlp = YtDlp {
            binary: PathBuf::from("/nonexistent/yt-dlp"),
            ..YtDlp::default()
        };
        assert!(matches!(
            ytdlp.version().await,
            Err(LinkError::YtDlpMissing)
        ));
    }
}
