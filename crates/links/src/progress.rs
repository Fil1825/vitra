//! yt-dlp's download output, reduced to events. Vitra asks yt-dlp for machine-readable lines
//! (see [`crate::download_args`]) instead of parsing its human progress bar.

use std::path::PathBuf;

/// Prefixes of the lines `download_args` asks yt-dlp to print.
pub(crate) const PROGRESS: &str = "VITRA-PROGRESS";
pub(crate) const POSTPROCESS: &str = "VITRA-POST";
pub(crate) const FILE: &str = "VITRA-FILE";

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadEvent {
    /// Bytes so far, out of `total` when known (exact or estimated).
    Progress { downloaded: u64, total: Option<u64> },
    /// A post-processing step started (extracting audio, embedding metadata, ...).
    Processing(String),
    /// The finished file.
    File(PathBuf),
}

fn number(field: Option<&str>) -> Option<u64> {
    // yt-dlp prints "NA" for missing values, and sometimes floats for estimates.
    field
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as u64)
}

/// Parses one line of yt-dlp's output; other lines (warnings, errors) give `None`.
pub fn parse_progress_line(line: &str) -> Option<DownloadEvent> {
    let line = line.trim_end_matches(['\r', '\n']);
    if let Some(path) = line
        .strip_prefix(FILE)
        .and_then(|rest| rest.strip_prefix(' '))
    {
        return (!path.is_empty()).then(|| DownloadEvent::File(PathBuf::from(path)));
    }
    let mut words = line.split(' ');
    match words.next()? {
        PROGRESS => {
            let _status = words.next();
            let downloaded = number(words.next())?;
            let total = number(words.next()).or_else(|| number(words.next()));
            Some(DownloadEvent::Progress { downloaded, total })
        }
        POSTPROCESS => match (words.next(), words.next()) {
            (Some("started"), Some(step)) => Some(DownloadEvent::Processing(step.to_owned())),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_progress_with_exact_or_estimated_totals() {
        // Lines recorded from a real download.
        assert_eq!(
            parse_progress_line("VITRA-PROGRESS downloading 130048 257152 NA"),
            Some(DownloadEvent::Progress {
                downloaded: 130_048,
                total: Some(257_152)
            })
        );
        assert_eq!(
            parse_progress_line("VITRA-PROGRESS downloading 1024 NA 5000000.5"),
            Some(DownloadEvent::Progress {
                downloaded: 1024,
                total: Some(5_000_000)
            })
        );
        assert_eq!(
            parse_progress_line("VITRA-PROGRESS downloading 1024 NA NA"),
            Some(DownloadEvent::Progress {
                downloaded: 1024,
                total: None
            })
        );
    }

    #[test]
    fn reads_post_processing_and_the_final_file() {
        assert_eq!(
            parse_progress_line("VITRA-POST started ExtractAudio"),
            Some(DownloadEvent::Processing("ExtractAudio".into()))
        );
        assert_eq!(
            parse_progress_line("VITRA-POST finished ExtractAudio"),
            None
        );
        assert_eq!(
            parse_progress_line("VITRA-FILE /music/Artist - Song (Live).mp3\n"),
            Some(DownloadEvent::File(PathBuf::from(
                "/music/Artist - Song (Live).mp3"
            )))
        );
    }

    #[test]
    fn ignores_everything_else() {
        for line in [
            "",
            "[download] Destination: x.webm",
            "ERROR: Unsupported URL",
            "VITRA-PROGRESS downloading NA NA NA",
            "VITRA-FILE ",
        ] {
            assert_eq!(parse_progress_line(line), None, "{line}");
        }
    }
}
