//! Windows (SMTC) and Linux (MPRIS) through `souvlaki`. Both take the cover as a file URL,
//! so the playing track's artwork is written to the cache folder.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use media::PreparedArtwork;
use souvlaki::{
    MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig,
    SeekDirection,
};
use tauri::{AppHandle, Manager};

use super::{Backend, Command, Info, Playback};

/// How far the "seek" buttons of some MPRIS clients jump.
const SEEK_STEP: f64 = 10.0;

pub(super) fn start(
    app: &AppHandle,
    handler: impl Fn(Command) + Send + Sync + 'static,
) -> Result<Box<dyn Backend>, String> {
    #[cfg(target_os = "windows")]
    let hwnd = {
        let window = app
            .get_webview_window(crate::mini::MAIN)
            .ok_or("no main window")?;
        Some(window.hwnd().map_err(|err| err.to_string())?.0)
    };
    #[cfg(not(target_os = "windows"))]
    let hwnd = None;
    let mut controls = MediaControls::new(PlatformConfig {
        dbus_name: "vitra",
        display_name: "Vitra",
        hwnd,
    })
    .map_err(|err| format!("{err:?}"))?;
    controls
        .attach(move |event| {
            if let Some(command) = command(event) {
                handler(command);
            }
        })
        .map_err(|err| format!("{err:?}"))?;
    let covers = app
        .path()
        .app_cache_dir()
        .map(|dir| dir.join("now-playing"))
        .map_err(|err| err.to_string())?;
    Ok(Box::new(SouvlakiBackend {
        controls,
        covers,
        cover: None,
    }))
}

fn command(event: MediaControlEvent) -> Option<Command> {
    let step = |direction| match direction {
        SeekDirection::Forward => 1.0,
        SeekDirection::Backward => -1.0,
    };
    Some(match event {
        MediaControlEvent::Play => Command::Play,
        MediaControlEvent::Pause => Command::Pause,
        MediaControlEvent::Toggle => Command::Toggle,
        MediaControlEvent::Next => Command::Next,
        MediaControlEvent::Previous => Command::Previous,
        MediaControlEvent::Stop => Command::Stop,
        MediaControlEvent::Seek(direction) => Command::SeekBy(step(direction) * SEEK_STEP),
        MediaControlEvent::SeekBy(direction, by) => {
            Command::SeekBy(step(direction) * by.as_secs_f64())
        }
        MediaControlEvent::SetPosition(MediaPosition(at)) => Command::SeekTo(at.as_secs_f64()),
        MediaControlEvent::Raise => Command::Raise,
        MediaControlEvent::SetVolume(_)
        | MediaControlEvent::OpenUri(_)
        | MediaControlEvent::Quit => return None,
    })
}

struct SouvlakiBackend {
    controls: MediaControls,
    /// Where the cover files go.
    covers: PathBuf,
    /// The cover written last: (artwork id, file).
    cover: Option<(String, PathBuf)>,
}

impl SouvlakiBackend {
    /// The artwork as a file URL, written once per cover.
    fn cover_url(&mut self, id: &str, artwork: &PreparedArtwork) -> Option<String> {
        if let Some((written, path)) = &self.cover
            && written == id
        {
            return Some(file_url(path));
        }
        if let Some((_, old)) = self.cover.take() {
            let _ = std::fs::remove_file(old);
        }
        let extension = match artwork.image.mime.as_deref() {
            Some("image/png") => "png",
            Some("image/webp") => "webp",
            _ => "jpg",
        };
        let path = self.covers.join(format!("{id}.{extension}"));
        let written = std::fs::create_dir_all(&self.covers)
            .and_then(|()| std::fs::write(&path, &artwork.image.bytes));
        if let Err(err) = written {
            log::info!("now playing: cannot write the cover: {err}");
            return None;
        }
        let url = file_url(&path);
        self.cover = Some((id.to_owned(), path));
        Some(url)
    }
}

/// souvlaki reads `file://` + the path on Windows and hands MPRIS the URL as it is.
fn file_url(path: &std::path::Path) -> String {
    format!("file://{}", path.display())
}

impl Backend for SouvlakiBackend {
    fn set_info(&mut self, info: Option<&Info>, artwork: Option<Arc<PreparedArtwork>>) {
        let Some(info) = info else {
            let _ = self.controls.set_metadata(MediaMetadata::default());
            return;
        };
        let cover = match (info.artwork.as_deref(), artwork.as_deref()) {
            (Some(id), Some(artwork)) => self.cover_url(id, artwork),
            _ => None,
        };
        let metadata = MediaMetadata {
            title: Some(&info.title),
            artist: info.artist.as_deref(),
            album: info.album.as_deref(),
            cover_url: cover.as_deref(),
            duration: info.duration.map(Duration::from_secs_f64),
        };
        if let Err(err) = self.controls.set_metadata(metadata) {
            log::info!("now playing: {err:?}");
        }
    }

    fn set_playback(&mut self, playback: Playback) {
        let at = |seconds: f64| Some(MediaPosition(Duration::from_secs_f64(seconds.max(0.0))));
        let playback = match playback {
            Playback::Stopped => MediaPlayback::Stopped,
            Playback::Paused { position } => MediaPlayback::Paused {
                progress: at(position),
            },
            Playback::Playing { position, .. } => MediaPlayback::Playing {
                progress: at(position),
            },
        };
        if let Err(err) = self.controls.set_playback(playback) {
            log::info!("now playing: {err:?}");
        }
    }

    /// MPRIS answers the position it was last given, and SMTC's timeline wants updates too.
    fn refresh(&self) -> Option<Duration> {
        Some(Duration::from_secs(1))
    }
}
