//! Runs the real yt-dlp against pages served from this test (no internet needed).
//! Opt-in: `VITRA_YTDLP=/path/to/yt-dlp cargo test -p links --test real_ytdlp`.
//! FFmpeg is taken from `VITRA_FFMPEG` or `PATH`. Relative paths are taken from the
//! workspace root (cargo runs the tests in `crates/links`).

mod common;

use std::path::{Path, PathBuf};

use links::ytdlp::YtDlp;
use links::{AudioFormat, DownloadEvent, DownloadOptions, LinkError, Resolved};

/// A 1×1 PNG.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// A path from the environment, relative ones from the workspace root.
fn env_path(name: &str) -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os(name)?);
    Some(if path.is_absolute() {
        path
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    })
}

fn ytdlp() -> Option<YtDlp> {
    let binary = env_path("VITRA_YTDLP")?;
    let ffmpeg = env_path("VITRA_FFMPEG").or_else(|| {
        std::env::split_paths(&std::env::var_os("PATH")?)
            .map(|dir| dir.join("ffmpeg"))
            .find(|path| path.is_file())
    });
    Some(YtDlp {
        binary,
        ffmpeg,
        ..YtDlp::default()
    })
}

/// Serves a song page, a two-song page, their cover and audio; returns the base URL.
fn serve() -> String {
    let flac = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../media/tests/fixtures/tagged.flac"),
    )
    .unwrap();
    common::serve(|base| {
        let page = |title: &str, body: &str| {
            format!(
                "<!doctype html><html><head><title>{title}</title>\
                 <meta property=\"og:title\" content=\"{title}\">\
                 <meta property=\"og:image\" content=\"{base}/cover.png\"></head>\
                 <body>{body}</body></html>"
            )
            .into_bytes()
        };
        vec![
            (
                "/song.html",
                page("Northern Song", "<audio src=\"one.flac\"></audio>"),
                "text/html",
            ),
            (
                "/album.html",
                page(
                    "Two Songs",
                    "<audio src=\"one.flac\"></audio><audio src=\"two.flac\"></audio>",
                ),
                "text/html",
            ),
            ("/cover.png", PNG.to_vec(), "image/png"),
            ("/one.flac", flac.clone(), "audio/flac"),
            ("/two.flac", flac, "audio/flac"),
        ]
    })
}

/// yt-dlp writes nothing into the working folder (cargo's is `crates/links`).
fn assert_no_images_here() {
    let images: Vec<_> = std::fs::read_dir(".")
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            let name = name.to_ascii_lowercase();
            [".png", ".jpg", ".webp", ".json"]
                .iter()
                .any(|ext| name.ends_with(ext))
        })
        .collect();
    assert!(images.is_empty(), "left behind: {images:?}");
}

#[tokio::test]
async fn resolves_a_page_with_its_thumbnail() {
    let Some(ytdlp) = ytdlp() else { return };
    let base = serve();
    let scratch = tempfile::tempdir().unwrap();
    let (resolved, thumbnail) = ytdlp
        .resolve(&format!("{base}/song.html"), scratch.path())
        .await
        .unwrap();
    let Resolved::Track(track) = resolved else {
        panic!("a track");
    };
    assert!(track.title.starts_with("Northern Song"));
    assert_eq!(track.url, format!("{base}/song.html"));
    assert_eq!(thumbnail.as_deref(), Some(PNG));
    // Its scratch directory is gone.
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
    assert_no_images_here();
}

#[tokio::test]
async fn resolves_and_enriches_a_playlist() {
    let Some(ytdlp) = ytdlp() else { return };
    let base = serve();
    let scratch = tempfile::tempdir().unwrap();
    let url = format!("{base}/album.html");
    let (resolved, thumbnail) = ytdlp.resolve(&url, scratch.path()).await.unwrap();
    let Resolved::Playlist(playlist) = resolved else {
        panic!("a playlist");
    };
    // The playlist's own thumbnail comes back instead of landing in the working folder.
    assert_eq!(thumbnail.as_deref(), Some(PNG));
    assert_no_images_here();
    assert_eq!(playlist.title, "Two Songs");
    let urls: Vec<_> = playlist.tracks.iter().map(|t| t.url.clone()).collect();
    assert_eq!(
        urls,
        [format!("{base}/one.flac"), format!("{base}/two.flac")]
    );

    let mut seen = Vec::new();
    let count = ytdlp
        .enrich(&url, scratch.path(), |track, _| seen.push(track.id))
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(seen.len(), 2);
    assert_no_images_here();
    assert_eq!(std::fs::read_dir(scratch.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn downloads_and_converts_with_progress() {
    let Some(ytdlp) = ytdlp() else { return };
    let base = serve();
    let out = tempfile::tempdir().unwrap();
    let options = DownloadOptions {
        format: AudioFormat::Mp3,
        mp3_bitrate: 192,
        dir: out.path().join("Music"),
        temp_dir: out.path().join("tmp"),
        file_stem: "Vitra Test - Aurora".into(),
        ffmpeg: ytdlp.ffmpeg.clone(),
    };
    let (_cancel, never) = tokio::sync::watch::channel(false);
    let mut events = Vec::new();
    let path = ytdlp
        .download(&format!("{base}/one.flac"), &options, never, |e| {
            events.push(e.clone())
        })
        .await
        .unwrap();
    assert_eq!(path, out.path().join("Music/Vitra Test - Aurora.mp3"));
    assert!(path.is_file());
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Progress { .. }))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DownloadEvent::Processing(step) if step == "ExtractAudio"))
    );
}

#[tokio::test]
async fn a_cancelled_download_stops() {
    let Some(ytdlp) = ytdlp() else { return };
    let base = serve();
    let out = tempfile::tempdir().unwrap();
    let options = DownloadOptions {
        format: AudioFormat::Flac,
        mp3_bitrate: 320,
        dir: out.path().join("Music"),
        temp_dir: out.path().join("tmp"),
        file_stem: "never".into(),
        ffmpeg: ytdlp.ffmpeg.clone(),
    };
    let (cancel, receiver) = tokio::sync::watch::channel(false);
    cancel.send(true).unwrap();
    let result = ytdlp
        .download(&format!("{base}/one.flac"), &options, receiver, |_| {})
        .await;
    assert!(matches!(result, Err(LinkError::Cancelled)));
    assert!(!out.path().join("Music/never.flac").exists());
}

#[tokio::test]
async fn reports_unsupported_links() {
    let Some(ytdlp) = ytdlp() else { return };
    let base = serve();
    let scratch = tempfile::tempdir().unwrap();
    let error = ytdlp
        .resolve(&format!("{base}/missing"), scratch.path())
        .await
        .unwrap_err();
    assert!(matches!(error, LinkError::Failed(_)), "{error:?}");
    assert!(!ytdlp.version().await.unwrap().is_empty());
}

#[tokio::test]
async fn finds_a_pages_audio_stream() {
    let Some(ytdlp) = ytdlp() else {
        eprintln!("VITRA_YTDLP not set: skipped");
        return;
    };
    let base = serve();
    let scratch = tempfile::tempdir().unwrap();
    let page = format!("{base}/song.html");
    let stream = ytdlp
        .audio_stream(&page, scratch.path())
        .await
        .expect("the page's audio");
    assert_eq!(stream.url, format!("{base}/one.flac"));
    // Described under the page's own address.
    assert_eq!(stream.track.map(|t| t.url), Some(page));
}
