//! What FFmpeg is asked to decode: a few slices of a file, or a stretch from the middle of a
//! stream, as mono 32-bit floats at the analysis rate on its standard output.

use std::ffi::{OsStr, OsString};

use crate::features::SAMPLE_RATE;

/// Seconds of each of a file's three slices.
pub const SLICE_SECONDS: f64 = 30.0;
/// The most a track is decoded: three slices, or the start of a short or unmeasured track.
pub const MAX_SECONDS: f64 = 90.0;
/// A stream's stretch from its middle: one request, a little longer than a slice.
pub const STREAM_SECONDS: f64 = 75.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slice {
    pub start: f64,
    pub seconds: f64,
}

/// Where to listen: three 30 s slices around 20 %, 50 % and 80 % of a track long enough for
/// them (verse, chorus, the later part: one slice alone is often an intro), else its first
/// minute and a half.
pub fn slices(duration: Option<f64>) -> Vec<Slice> {
    match duration.filter(|d| d.is_finite() && *d >= MAX_SECONDS) {
        Some(duration) => [0.2, 0.5, 0.8]
            .iter()
            .map(|share| Slice {
                start: (duration * share - SLICE_SECONDS / 2.0)
                    .clamp(0.0, duration - SLICE_SECONDS)
                    .floor(),
                seconds: SLICE_SECONDS,
            })
            .collect(),
        None => vec![Slice {
            start: 0.0,
            seconds: MAX_SECONDS,
        }],
    }
}

/// The middle of a stream of `duration` (unknown: from a minute in).
pub fn stream_slice(duration: Option<f64>) -> Slice {
    let start = duration
        .filter(|d| d.is_finite() && *d > 0.0)
        .map_or(60.0, |d| ((d - STREAM_SECONDS) / 2.0).max(0.0).floor());
    Slice {
        start,
        seconds: STREAM_SECONDS,
    }
}

fn base_args() -> Vec<OsString> {
    [
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-threads",
        "1",
    ]
    .iter()
    .map(OsString::from)
    .collect()
}

/// Every input's first audio stream converted to the analysis format, one after another.
fn output_args(inputs: usize) -> Vec<OsString> {
    let format = format!("aformat=sample_fmts=flt:sample_rates={SAMPLE_RATE}:channel_layouts=mono");
    let mut graph: Vec<String> = (0..inputs)
        .map(|i| format!("[{i}:a:0]{format}[s{i}]"))
        .collect();
    let joined: String = (0..inputs).map(|i| format!("[s{i}]")).collect();
    graph.push(format!("{joined}concat=n={inputs}:v=0:a=1[out]"));
    [
        "-filter_complex",
        &graph.join(";"),
        "-map",
        "[out]",
        "-f",
        "f32le",
        "-",
    ]
    .iter()
    .map(OsString::from)
    .collect()
}

/// Arguments decoding `slices` of the file at `path` (each slice its own input, so FFmpeg
/// seeks rather than decodes its way there).
pub fn file_args(path: &OsStr, slices: &[Slice]) -> Vec<OsString> {
    let mut args = base_args();
    for slice in slices {
        args.extend(seek(slice));
        args.push("-i".into());
        args.push(path.to_os_string());
    }
    args.extend(output_args(slices.len()));
    args
}

/// Arguments decoding a stretch of the stream at `url`, sending `headers` (what yt-dlp says
/// the site expects). `-ss` before `-i` makes FFmpeg ask for that range only.
pub fn stream_args(url: &str, headers: &[(String, String)], slice: Slice) -> Vec<OsString> {
    let mut args = base_args();
    // Sidecars use different TLS backends/defaults. Always authenticate HTTPS inputs.
    // Plain HTTP does not accept this protocol option (including internet radio).
    if url
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        args.extend(["-tls_verify".into(), "1".into()]);
    }
    if !headers.is_empty() {
        let joined: String = headers
            .iter()
            .filter(|(name, value)| !name.contains(['\r', '\n']) && !value.contains(['\r', '\n']))
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        args.push("-headers".into());
        args.push(joined.into());
    }
    args.extend(seek(&slice));
    args.push("-i".into());
    args.push(url.into());
    args.extend(output_args(1));
    args
}

fn seek(slice: &Slice) -> Vec<OsString> {
    let mut args = Vec::new();
    if slice.start > 0.0 {
        args.push("-ss".into());
        args.push(format!("{}", slice.start).into());
    }
    args.push("-t".into());
    args.push(format!("{}", slice.seconds).into());
    args
}

/// FFmpeg's raw output as samples (a trailing partial sample is dropped).
pub fn samples(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn listens_to_three_parts_of_a_song() {
        let parts = slices(Some(200.0));
        let starts: Vec<f64> = parts.iter().map(|s| s.start).collect();
        assert_eq!(starts, vec![25.0, 85.0, 145.0]);
        // The last slice ends before the track does, however short (a 90 s track).
        let short = slices(Some(90.0));
        assert!(short.iter().all(|s| s.start + s.seconds <= 90.0));
        // Shorter or unknown: the start, up to a minute and a half.
        assert_eq!(
            slices(Some(60.0)),
            vec![Slice {
                start: 0.0,
                seconds: 90.0
            }]
        );
        assert_eq!(slices(None), slices(Some(f64::NAN)));
    }

    #[test]
    fn decodes_each_slice_as_its_own_input() {
        let args = strings(&file_args(OsStr::new("/m/a b.flac"), &slices(Some(200.0))));
        assert_eq!(args.iter().filter(|a| *a == "/m/a b.flac").count(), 3);
        let at = args.iter().position(|a| a == "-ss").unwrap();
        assert_eq!(
            &args[at..at + 6],
            ["-ss", "25", "-t", "30", "-i", "/m/a b.flac"]
        );
        let graph = &args[args.iter().position(|a| a == "-filter_complex").unwrap() + 1];
        assert!(
            graph.ends_with("[s0][s1][s2]concat=n=3:v=0:a=1[out]"),
            "{graph}"
        );
        assert!(graph.contains("sample_rates=22050:channel_layouts=mono"));
        assert_eq!(args.last().unwrap(), "-");
        assert!(args.windows(2).any(|w| w == ["-threads", "1"]));
    }

    #[test]
    fn a_stream_is_read_from_its_middle_with_its_headers() {
        let headers = vec![
            ("User-Agent".to_owned(), "Mozilla/5.0".to_owned()),
            ("Bad".to_owned(), "x\r\nInjected: 1".to_owned()),
        ];
        let slice = stream_slice(Some(215.0));
        assert_eq!(
            slice,
            Slice {
                start: 70.0,
                seconds: 75.0
            }
        );
        let args = strings(&stream_args("https://a.example/s", &headers, slice));
        let at = args.iter().position(|a| a == "-headers").unwrap();
        assert_eq!(args[at + 1], "User-Agent: Mozilla/5.0\r\n");
        let input = args.iter().position(|a| a == "-i").unwrap();
        let tls = args.iter().position(|a| a == "-tls_verify").unwrap();
        assert_eq!(args[tls + 1], "1");
        assert!(tls < input);
        assert_eq!(
            &args[input - 4..=input + 1],
            ["-ss", "70", "-t", "75", "-i", "https://a.example/s"]
        );
        assert_eq!(stream_slice(None).start, 60.0);
    }

    #[test]
    fn plain_http_does_not_receive_https_only_options() {
        let args = strings(&stream_args(
            "http://radio.example/live",
            &[],
            stream_slice(None),
        ));
        assert!(!args.iter().any(|arg| arg == "-tls_verify"));
    }

    #[test]
    fn reads_little_endian_floats() {
        let bytes: Vec<u8> = [0.5f32, -1.0]
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .chain([1])
            .collect();
        assert_eq!(samples(&bytes), vec![0.5, -1.0]);
    }
}
