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

/// YouTube Music's search for `query`, its "Songs" section (yt-dlp's
/// `youtube:music:search_url` reads the section from the fragment).
pub fn music_search_url(query: &str) -> String {
    let query: String = url::form_urlencoded::byte_serialize(query.trim().as_bytes()).collect();
    format!("https://music.youtube.com/search?q={query}#songs")
}

/// YouTube Music's radio of a song: the songs it plays after it (its "mix" list).
pub fn radio_url(video_id: &str) -> String {
    format!("https://music.youtube.com/watch?v={video_id}&list=RDAMVM{video_id}")
}

/// The video id of a YouTube or YouTube Music link (`watch?v=`, `youtu.be/`, `shorts/`).
pub fn youtube_id(link: &str) -> Option<String> {
    let parsed = url::Url::parse(link).ok()?;
    let host = parsed.host_str()?.trim_start_matches("www.");
    let id = match host {
        "youtu.be" => parsed.path_segments()?.next().map(str::to_owned),
        "youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            let mut segments = parsed.path_segments()?;
            match segments.next() {
                Some("watch") => parsed
                    .query_pairs()
                    .find(|(key, _)| key == "v")
                    .map(|(_, value)| value.into_owned()),
                Some("shorts" | "embed" | "live") => segments.next().map(str::to_owned),
                _ => None,
            }
        }
        _ => None,
    }?;
    // YouTube's ids are 11 URL-safe characters.
    (id.len() == 11
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'))
    .then_some(id)
}

/// A radio's songs without the one it is the radio of, at most `limit`.
fn without_seed(tracks: Vec<LinkTrack>, video_id: &str, limit: usize) -> Vec<LinkTrack> {
    tracks
        .into_iter()
        .filter(|track| {
            track.id.as_deref() != Some(video_id)
                && youtube_id(&track.url).as_deref() != Some(video_id)
        })
        .take(limit)
        .collect()
}

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

/// Where a link's audio streams from, for reading a stretch of it without playing it: the
/// media address yt-dlp chose, the headers the site expects with it, and the track's length.
/// The same run describes the track in full, so a link known only from a listing (a radio's
/// song) gets its details and thumbnail without asking the site again.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub duration: Option<f64>,
    /// The track as the page describes it, under the link it was asked for.
    pub track: Option<LinkTrack>,
    pub thumbnail: Option<Vec<u8>>,
}

/// Reads `-J -f bestaudio/best` output for `requested`: one chosen format, so its address is
/// the info's own.
pub fn parse_stream(json: &str, requested: &str) -> Result<AudioStream> {
    let info: serde_json::Value =
        serde_json::from_str(json).map_err(|err| LinkError::Failed(format!("bad JSON: {err}")))?;
    let url = info
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .ok_or_else(|| LinkError::Failed("no audio address".into()))?
        .to_owned();
    let mut headers: Vec<(String, String)> = info
        .get("http_headers")
        .and_then(serde_json::Value::as_object)
        .map(|map| {
            map.iter()
                .filter_map(|(name, value)| Some((name.clone(), value.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    headers.sort();
    let track = match parse_info(json, requested) {
        Ok(Resolved::Track(track)) => Some(LinkTrack {
            // The library knows the link by the address it was given.
            url: requested.to_owned(),
            ..track
        }),
        _ => None,
    };
    Ok(AudioStream {
        url,
        headers,
        duration: info
            .get("duration")
            .and_then(serde_json::Value::as_f64)
            .filter(|d| d.is_finite() && *d > 0.0),
        track,
        thumbnail: None,
    })
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

/// Why a site can't be asked right now, though it may answer later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outage {
    /// "Too many requests": the site wants us to slow down.
    RateLimited,
    /// No connection, a timeout, the site's own trouble (5xx).
    Unreachable,
}

/// Whether yt-dlp's error is the network's or the site's for now (worth waiting and asking
/// again) rather than about the page itself (gone, private, not a page yt-dlp reads).
pub fn outage(message: &str) -> Option<Outage> {
    let lower = message.to_lowercase();
    if lower.contains("http error 429") || lower.contains("too many requests") {
        return Some(Outage::RateLimited);
    }
    // A page that isn't there answers with 4xx: that's the page, not the network.
    if (400..500).any(|code| lower.contains(&format!("http error {code}"))) {
        return None;
    }
    const NETWORK: &[&str] = &[
        "http error 5",
        "urlopen error",
        "connection reset",
        "connection refused",
        "connection aborted",
        "remote end closed",
        "timed out",
        "name resolution",
        "name or service not known",
        "nodename nor servname",
        "network is unreachable",
        "no route to host",
        "temporarily unavailable",
        "ssl:",
        "eof occurred",
    ];
    NETWORK
        .iter()
        .any(|sign| lower.contains(sign))
        .then_some(Outage::Unreachable)
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
        crate::tls::configure(command.as_std_mut())?;
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

    /// The songs YouTube Music finds for `query`, best first: its "Songs" results (the
    /// labels' audio tracks, not videos), listed without opening each one, so they have a
    /// title and a link only ([`Self::describe`] tells the rest).
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<LinkTrack>> {
        self.listing(&music_search_url(query), limit).await
    }

    /// Songs like the video `video_id` from YouTube Music's radio of it (the song itself
    /// left out), listed without opening each one.
    pub async fn radio(&self, video_id: &str, limit: usize) -> Result<Vec<LinkTrack>> {
        // The radio starts with its song.
        let tracks = self.listing(&radio_url(video_id), limit + 1).await?;
        Ok(without_seed(tracks, video_id, limit))
    }

    /// The first `limit` entries of a list, without opening each one.
    async fn listing(&self, url: &str, limit: usize) -> Result<Vec<LinkTrack>> {
        let mut command = self.command();
        command
            .args(["-J", "--flat-playlist", "--playlist-end"])
            .arg(limit.max(1).to_string())
            .arg("--")
            .arg(url);
        let output = Self::output(command, RESOLVE_TIMEOUT).await?;
        Ok(
            match parse_info(&String::from_utf8_lossy(&output.stdout), url)? {
                Resolved::Playlist(playlist) => playlist.tracks,
                Resolved::Track(track) => vec![track],
            },
        )
    }

    /// Writes the cookies of the browser chosen in the settings into `file` (Netscape format),
    /// for requests Vitra makes itself (Yandex Music's private playlists). yt-dlp saves its
    /// cookie jar into `--cookies` when it exits, whatever the run did, so the run asks a
    /// local address that refuses at once and nothing goes out.
    pub async fn export_cookies(&self, file: &Path) -> Result<()> {
        if self.cookies_from_browser.is_none() {
            return Err(LinkError::Failed("no browser chosen for cookies".into()));
        }
        let mut command = self.command();
        command.arg("--cookies").arg(file).args([
            "--simulate",
            "--no-playlist",
            "--",
            "http://127.0.0.1:9/",
        ]);
        match Self::output(command, RESOLVE_TIMEOUT).await {
            Ok(_) | Err(LinkError::Failed(_)) => {}
            Err(err) => return Err(err),
        }
        match std::fs::metadata(file) {
            Ok(meta) if meta.len() > 0 => Ok(()),
            _ => Err(LinkError::Cookies("yt-dlp wrote no cookies".into())),
        }
    }

    /// The address of `url`'s best audio and how to ask for it (no audio is downloaded),
    /// with the track's description and thumbnail. `scratch` is a directory for temporary
    /// files.
    pub async fn audio_stream(&self, url: &str, scratch: &Path) -> Result<AudioStream> {
        let dir = scratch_dir(scratch)?;
        let mut command = self.command();
        command
            .args(["-J", "--no-playlist", "-f", "bestaudio/best"])
            .args(["--no-simulate", "--skip-download", "--write-thumbnail"])
            .args(thumbnail_templates(&dir))
            .current_dir(&dir)
            .arg("--")
            .arg(url);
        let result = Self::output(command, RESOLVE_TIMEOUT)
            .await
            .and_then(|output| {
                let stream = parse_stream(&String::from_utf8_lossy(&output.stdout), url)?;
                Ok(AudioStream {
                    thumbnail: thumbnail_in(&dir, None),
                    ..stream
                })
            });
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    /// One track's complete metadata (artists, album, length) and its thumbnail, in one run.
    /// `scratch` is a directory for temporary files.
    pub async fn describe(
        &self,
        url: &str,
        scratch: &Path,
    ) -> Result<(LinkTrack, Option<Vec<u8>>)> {
        let dir = scratch_dir(scratch)?;
        let mut command = self.command();
        command
            .args(["-J", "--no-playlist"])
            .args(["--no-simulate", "--skip-download", "--write-thumbnail"])
            .args(thumbnail_templates(&dir))
            .current_dir(&dir)
            .arg("--")
            .arg(url);
        let result = Self::output(command, RESOLVE_TIMEOUT)
            .await
            .and_then(
                |output| match parse_info(&String::from_utf8_lossy(&output.stdout), url)? {
                    Resolved::Track(track) => Ok((track, thumbnail_in(&dir, None))),
                    Resolved::Playlist(_) => Err(LinkError::Failed("a list, not a track".into())),
                },
            );
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
        crate::tls::configure(command.as_std_mut())?;
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
        crate::tls::configure(command.as_std_mut())?;
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
    fn all_operations_use_the_configured_deno_even_without_cookies() {
        let signed_in = YtDlp {
            binary: "yt-dlp".into(),
            deno: Some("/sidecars/a space/deno".into()),
            cookies_from_browser: Some("firefox".into()),
            ..YtDlp::default()
        };
        let plain = YtDlp {
            cookies_from_browser: None,
            ..signed_in.clone()
        };
        for runner in [&signed_in, &plain] {
            let command = runner.command();
            let args: Vec<_> = command.as_std().get_args().collect();
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["--js-runtimes", "deno:/sidecars/a space/deno"])
            );
        }
        let command = plain.command();
        assert!(
            !command
                .as_std()
                .get_args()
                .any(|arg| arg == "--cookies-from-browser")
        );
    }

    #[test]
    fn a_stream_is_its_address_and_headers() {
        let page = "https://music.youtube.com/watch?v=dQw4w9WgXcQ";
        let stream =
            parse_stream(include_str!("../tests/fixtures/youtube-stream.json"), page).unwrap();
        assert!(
            stream
                .url
                .starts_with("https://rr3---sn-example.googlevideo.com/videoplayback")
        );
        assert_eq!(stream.duration, Some(213.0));
        assert_eq!(stream.headers.len(), 4);
        assert!(
            stream
                .headers
                .iter()
                .any(|(name, value)| name == "User-Agent" && value.starts_with("Mozilla/5.0"))
        );
        // The same run describes the track, under the address the library knows.
        let track = stream.track.expect("the track's details");
        assert_eq!(track.url, page);
        assert_eq!(track.title, "Northern Song");
        assert_eq!(track.duration, Some(213.0));
        // No address (a page that only lists), or not a web one: nothing to read.
        assert!(parse_stream(r#"{"id": "x", "title": "y"}"#, page).is_err());
        assert!(parse_stream(r#"{"url": "file:///etc/passwd"}"#, page).is_err());
        assert!(parse_stream("not json", page).is_err());
    }

    #[test]
    fn youtube_ids_come_from_every_kind_of_link() {
        for link in [
            "https://www.youtube.com/watch?v=MgNrAu2pzNs",
            "https://music.youtube.com/watch?v=MgNrAu2pzNs&list=RDAMVMMgNrAu2pzNs",
            "https://youtu.be/MgNrAu2pzNs?t=4",
            "https://m.youtube.com/shorts/MgNrAu2pzNs",
        ] {
            assert_eq!(youtube_id(link).as_deref(), Some("MgNrAu2pzNs"), "{link}");
        }
        for link in [
            "https://www.youtube.com/playlist?list=PL0",
            "https://soundcloud.com/forss/flickermood",
            "https://www.youtube.com/watch?v=short",
            "https://evil.example/watch?v=MgNrAu2pzNs",
            "/Music/song.flac",
        ] {
            assert_eq!(youtube_id(link), None, "{link}");
        }
        assert_eq!(
            radio_url("MgNrAu2pzNs"),
            "https://music.youtube.com/watch?v=MgNrAu2pzNs&list=RDAMVMMgNrAu2pzNs"
        );
    }

    #[test]
    fn a_radio_lists_the_songs_after_its_own() {
        let url = radio_url("MgNrAu2pzNs");
        let Ok(Resolved::Playlist(playlist)) =
            parse_info(include_str!("../tests/fixtures/youtube-radio.json"), &url)
        else {
            panic!("a radio is a list");
        };
        let tracks = without_seed(playlist.tracks, "MgNrAu2pzNs", 2);
        let titles: Vec<&str> = tracks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Strobe", "Midnight City"]);
        assert_eq!(tracks[0].artist.as_deref(), Some("deadmau5"));
        assert_eq!(tracks[0].duration, Some(637.0));
    }

    /// `scripts/record-ytdlp-fixtures.sh` records a real radio, if it was run.
    #[test]
    fn a_recorded_radio_lists_other_songs() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/youtube-radio-recorded.json");
        let Ok(json) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(Resolved::Playlist(playlist)) = parse_info(&json, &radio_url("MgNrAu2pzNs")) else {
            panic!("a recorded radio is a list");
        };
        let seed = playlist
            .tracks
            .first()
            .and_then(|t| youtube_id(&t.url))
            .expect("the radio starts with its song");
        let tracks = without_seed(playlist.tracks, &seed, 25);
        assert!(tracks.len() >= 5, "{} songs", tracks.len());
        assert!(tracks.iter().all(|t| youtube_id(&t.url).is_some()));
    }

    #[test]
    fn music_search_urls() {
        assert_eq!(
            music_search_url(" Кино Группа крови "),
            "https://music.youtube.com/search?q=%D0%9A%D0%B8%D0%BD%D0%BE+%D0%93%D1%80%D1%83%D0%BF%D0%BF%D0%B0+%D0%BA%D1%80%D0%BE%D0%B2%D0%B8#songs"
        );
        assert_eq!(
            music_search_url("AC/DC & co #1"),
            "https://music.youtube.com/search?q=AC%2FDC+%26+co+%231#songs"
        );
    }

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
    fn outages_are_told_from_pages() {
        assert_eq!(
            outage(
                "[youtube:music:search_url] x: Unable to download API page: HTTP Error 429: Too Many Requests"
            ),
            Some(Outage::RateLimited)
        );
        assert_eq!(
            outage(
                "[youtube] x: Unable to download webpage: <urlopen error [Errno 8] nodename nor servname provided, or not known>"
            ),
            Some(Outage::Unreachable)
        );
        assert_eq!(
            outage("Unable to download webpage: HTTP Error 503: Service Unavailable"),
            Some(Outage::Unreachable)
        );
        assert_eq!(
            outage("[youtube] x: Unable to download webpage: HTTP Error 404: Not Found"),
            None
        );
        assert_eq!(outage("[youtube] abc: Video unavailable"), None);
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
