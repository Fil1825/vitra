//! Downloading a link into the library: yt-dlp's arguments and a job's life cycle.

use std::ffi::OsString;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::progress::{DownloadEvent, FILE, POSTPROCESS, PROGRESS};

/// What the downloaded file is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AudioFormat {
    /// The site's own stream without re-encoding (usually Opus or AAC/M4A): best quality.
    #[default]
    Original,
    Mp3,
    Flac,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadOptions {
    pub format: AudioFormat,
    /// kbit/s, for MP3.
    pub mp3_bitrate: u32,
    /// Where the file ends up.
    pub dir: PathBuf,
    /// Where yt-dlp keeps partial files; deleting it cleans up a cancelled download.
    pub temp_dir: PathBuf,
    /// File name without extension (see [`crate::file_stem`]).
    pub file_stem: String,
    pub ffmpeg: Option<PathBuf>,
}

/// MP3 bitrates offered in the settings.
pub const MP3_BITRATES: &[u32] = &[128, 192, 256, 320];

/// yt-dlp arguments (after the common ones, see [`crate::ytdlp::YtDlp`]) that download
/// `url` as audio, embed the cover and tags, and print machine-readable progress.
pub fn download_args(url: &str, options: &DownloadOptions) -> Vec<OsString> {
    let mut args: Vec<OsString> = Vec::new();
    let mut push = |arg: &str| args.push(arg.into());
    push("--no-playlist");
    push("-f");
    push("bestaudio/best");
    push("-x");
    match options.format {
        AudioFormat::Original => {}
        AudioFormat::Mp3 => {
            let bitrate = if MP3_BITRATES.contains(&options.mp3_bitrate) {
                options.mp3_bitrate
            } else {
                320
            };
            push("--audio-format");
            push("mp3");
            push("--audio-quality");
            push(&format!("{bitrate}K"));
        }
        AudioFormat::Flac => {
            push("--audio-format");
            push("flac");
        }
    }
    for arg in [
        "--embed-metadata",
        "--embed-thumbnail",
        // Some formats can't take WebP covers; JPEG works everywhere.
        "--convert-thumbnails",
        "jpg",
        // The file's date is when it was downloaded, not when the video was uploaded, so
        // "Recently Added" and folder scans see it as new.
        "--no-mtime",
        "--newline",
        "--progress",
        "--quiet",
        "--progress-template",
    ] {
        push(arg);
    }
    push(&format!(
        "download:{PROGRESS} %(progress.status)s %(progress.downloaded_bytes)s \
         %(progress.total_bytes)s %(progress.total_bytes_estimate)s"
    ));
    push("--progress-template");
    push(&format!(
        "postprocess:{POSTPROCESS} %(progress.status)s %(progress.postprocessor)s"
    ));
    push("--print");
    push(&format!("after_move:{FILE} %(filepath)s"));
    let mut path_arg = |kind: &str, path: &std::path::Path| {
        let mut value = OsString::from(kind);
        value.push(path);
        args.push("-P".into());
        args.push(value);
    };
    path_arg("home:", &options.dir);
    path_arg("temp:", &options.temp_dir);
    if let Some(ffmpeg) = &options.ffmpeg {
        args.push("--ffmpeg-location".into());
        args.push(ffmpeg.into());
    }
    // yt-dlp's own template syntax must not interpret the stem.
    args.push("-o".into());
    args.push(format!("{}.%(ext)s", options.file_stem.replace('%', "%%")).into());
    args.push("--".into());
    args.push(url.into());
    args
}

/// Where a download is in its life.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DownloadStatus {
    Queued,
    /// `progress` is 0–1 when the size is known.
    Downloading {
        progress: Option<f64>,
    },
    /// Converting, tagging, embedding the cover.
    Processing,
    Done {
        path: String,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

impl DownloadStatus {
    /// Still to run or running.
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Downloading { .. } | Self::Processing
        )
    }

    pub fn can_retry(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Cancelled)
    }

    /// The job's process started.
    pub fn started(&self) -> Option<Self> {
        matches!(self, Self::Queued).then_some(Self::Downloading { progress: None })
    }

    /// Follows yt-dlp's output. A file event alone doesn't finish the job: the app still has
    /// to move it into the library, then calls [`Self::finished`].
    pub fn on_event(&self, event: &DownloadEvent) -> Option<Self> {
        if !matches!(self, Self::Downloading { .. } | Self::Processing) {
            return None;
        }
        match event {
            DownloadEvent::Progress { downloaded, total } => {
                let progress = total
                    .filter(|total| *total > 0)
                    .map(|total| (*downloaded as f64 / total as f64).clamp(0.0, 1.0));
                Some(Self::Downloading { progress })
            }
            // The cover is converted before the audio downloads; that isn't processing yet.
            DownloadEvent::Processing(step) if step == "ThumbnailsConvertor" => None,
            DownloadEvent::Processing(_) => Some(Self::Processing),
            DownloadEvent::File(_) => None,
        }
    }

    pub fn finished(&self, path: String) -> Option<Self> {
        self.is_active().then_some(Self::Done { path })
    }

    pub fn failed(&self, error: String) -> Option<Self> {
        self.is_active().then_some(Self::Failed { error })
    }

    pub fn cancelled(&self) -> Option<Self> {
        self.is_active().then_some(Self::Cancelled)
    }

    pub fn retried(&self) -> Option<Self> {
        self.can_retry().then_some(Self::Queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn options(format: AudioFormat) -> DownloadOptions {
        let drive = if cfg!(windows) { "C:" } else { "" };
        DownloadOptions {
            format,
            mp3_bitrate: 256,
            dir: PathBuf::from(format!("{drive}/music/Vitra")),
            temp_dir: PathBuf::from(format!("{drive}/cache/dl/1")),
            file_stem: "Artist - 100% Song".into(),
            ffmpeg: Some(PathBuf::from(format!("{drive}/bin/ffmpeg"))),
        }
    }

    fn args(format: AudioFormat) -> Vec<String> {
        download_args("https://youtu.be/x", &options(format))
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect()
    }

    fn has_pair(args: &[String], key: &str, value: &str) -> bool {
        args.windows(2).any(|w| w[0] == key && w[1] == value)
    }

    #[test]
    fn original_format_keeps_the_stream() {
        let args = args(AudioFormat::Original);
        assert!(args.contains(&"-x".to_owned()));
        assert!(!args.contains(&"--audio-format".to_owned()));
        assert!(has_pair(&args, "-f", "bestaudio/best"));
        assert!(args.contains(&"--embed-thumbnail".to_owned()));
        assert!(args.contains(&"--embed-metadata".to_owned()));
        assert!(args.contains(&"--no-mtime".to_owned()));
    }

    #[test]
    fn mp3_and_flac_convert() {
        let mp3 = args(AudioFormat::Mp3);
        assert!(has_pair(&mp3, "--audio-format", "mp3"));
        assert!(has_pair(&mp3, "--audio-quality", "256K"));
        let flac = args(AudioFormat::Flac);
        assert!(has_pair(&flac, "--audio-format", "flac"));
        assert!(!flac.contains(&"--audio-quality".to_owned()));

        let mut odd = options(AudioFormat::Mp3);
        odd.mp3_bitrate = 999;
        let odd: Vec<String> = download_args("u", &odd)
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert!(has_pair(&odd, "--audio-quality", "320K"));
    }

    #[test]
    fn paths_names_and_the_url_are_placed_safely() {
        let args = args(AudioFormat::Original);
        let drive = if cfg!(windows) { "C:" } else { "" };
        assert!(has_pair(&args, "-P", &format!("home:{drive}/music/Vitra")));
        assert!(has_pair(&args, "-P", &format!("temp:{drive}/cache/dl/1")));
        assert!(has_pair(
            &args,
            "--ffmpeg-location",
            &format!("{drive}/bin/ffmpeg")
        ));
        // `%` would start a yt-dlp template field.
        assert!(has_pair(&args, "-o", "Artist - 100%% Song.%(ext)s"));
        // The URL comes after `--`, so a link can never be read as an option.
        assert_eq!(&args[args.len() - 2..], ["--", "https://youtu.be/x"]);
        assert!(Path::new(&options(AudioFormat::Flac).dir).is_absolute());
    }

    #[test]
    fn a_job_runs_through_its_states() {
        let queued = DownloadStatus::Queued;
        let running = queued.started().unwrap();
        assert_eq!(running, DownloadStatus::Downloading { progress: None });
        let half = running
            .on_event(&DownloadEvent::Progress {
                downloaded: 50,
                total: Some(100),
            })
            .unwrap();
        assert_eq!(
            half,
            DownloadStatus::Downloading {
                progress: Some(0.5)
            }
        );
        // Recorded order: the cover is converted before the audio downloads.
        assert_eq!(
            half.on_event(&DownloadEvent::Processing("ThumbnailsConvertor".into())),
            None
        );
        let processing = half
            .on_event(&DownloadEvent::Processing("ExtractAudio".into()))
            .unwrap();
        assert_eq!(processing, DownloadStatus::Processing);
        let done = processing.finished("/music/a.opus".into()).unwrap();
        assert!(!done.is_active());
        assert!(!done.can_retry());
        assert_eq!(done.cancelled(), None);
        assert_eq!(done.started(), None);
    }

    #[test]
    fn failures_and_cancellations_can_be_retried() {
        let running = DownloadStatus::Queued.started().unwrap();
        let failed = running.failed("HTTP Error 403".into()).unwrap();
        assert!(failed.can_retry());
        assert_eq!(failed.retried(), Some(DownloadStatus::Queued));
        assert_eq!(failed.failed("again".into()), None);

        let cancelled = DownloadStatus::Queued.cancelled().unwrap();
        assert_eq!(cancelled.retried(), Some(DownloadStatus::Queued));
        assert_eq!(
            cancelled.on_event(&DownloadEvent::Processing("x".into())),
            None
        );
        assert_eq!(DownloadStatus::Queued.retried(), None);
    }

    #[test]
    fn serializes_for_the_frontend() {
        let json = serde_json::to_value(DownloadStatus::Downloading {
            progress: Some(0.25),
        })
        .unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "state": "downloading", "progress": 0.25 })
        );
        assert_eq!(
            serde_json::to_value(AudioFormat::Mp3).unwrap(),
            serde_json::json!("mp3")
        );
    }
}
