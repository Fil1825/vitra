//! macOS: `MPNowPlayingInfoCenter` shows the track, `MPRemoteCommandCenter` brings the
//! buttons, media keys (F7–F9, the Touch Bar) and headphone buttons. AppKit wants all of it
//! on the main thread, so every call is posted there; the dictionary the system shows is
//! kept there too, and the playback values are written into it as they change.

use std::cell::{Cell, RefCell};
use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;
use media::PreparedArtwork;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::NSImage;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSData, NSMutableDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumTitle,
    MPMediaItemPropertyArtist, MPMediaItemPropertyArtwork, MPMediaItemPropertyPlaybackDuration,
    MPMediaItemPropertyTitle, MPNowPlayingInfoCenter, MPNowPlayingInfoMediaType,
    MPNowPlayingInfoPropertyElapsedPlaybackTime, MPNowPlayingInfoPropertyIsLiveStream,
    MPNowPlayingInfoPropertyMediaType, MPNowPlayingInfoPropertyPlaybackRate,
    MPNowPlayingPlaybackState, MPRemoteCommand, MPRemoteCommandCenter, MPRemoteCommandEvent,
    MPRemoteCommandHandlerStatus,
};
use tauri::AppHandle;

use super::{Backend, Command, Info, Playback};

type Handler = Arc<dyn Fn(Command) + Send + Sync>;

/// Registers the command handlers; the returned backend publishes the track.
pub(super) fn start(
    app: AppHandle,
    handler: impl Fn(Command) + Send + Sync + 'static,
) -> Result<Box<dyn Backend>, String> {
    let handler: Handler = Arc::new(handler);
    app.run_on_main_thread(move || register(&handler))
        .map_err(|err| err.to_string())?;
    Ok(Box::new(MacBackend { app }))
}

struct MacBackend {
    app: AppHandle,
}

impl Backend for MacBackend {
    fn set_info(&mut self, info: Option<&Info>, artwork: Option<Arc<PreparedArtwork>>) {
        let info = info.cloned();
        if let Err(err) = self
            .app
            .run_on_main_thread(move || publish_info(info.as_ref(), artwork.as_deref()))
        {
            log::warn!("now playing: {err}");
        }
    }

    fn set_playback(&mut self, playback: Playback) {
        if let Err(err) = self
            .app
            .run_on_main_thread(move || publish_playback(playback))
        {
            log::warn!("now playing: {err}");
        }
    }
}

thread_local! {
    /// What the system shows now (main thread only).
    static SHOWN: RefCell<Option<Retained<NSMutableDictionary<NSString, AnyObject>>>> =
        const { RefCell::new(None) };
    static PLAYBACK: Cell<Playback> = const { Cell::new(Playback::Stopped) };
}

fn register(handler: &Handler) {
    // SAFETY (all `unsafe` blocks below): MediaPlayer's own API, called on the main thread
    // as AppKit wants, with valid objects.
    #[allow(unsafe_code)]
    let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };
    let plain = |command: Retained<MPRemoteCommand>, what: Command| {
        let handler = Arc::clone(handler);
        let block = RcBlock::new(move |_event: NonNull<MPRemoteCommandEvent>| {
            handler(what);
            MPRemoteCommandHandlerStatus::Success
        });
        #[allow(unsafe_code)]
        unsafe {
            command.setEnabled(true);
            command.addTargetWithHandler(&block);
        }
    };
    #[allow(unsafe_code)]
    unsafe {
        plain(center.playCommand(), Command::Play);
        plain(center.pauseCommand(), Command::Pause);
        plain(center.togglePlayPauseCommand(), Command::Toggle);
        plain(center.stopCommand(), Command::Stop);
        plain(center.nextTrackCommand(), Command::Next);
        plain(center.previousTrackCommand(), Command::Previous);
    }

    // Scrubbing the widget's progress bar.
    let handler = Arc::clone(handler);
    let block = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
        #[allow(unsafe_code)]
        let event: &MPRemoteCommandEvent = unsafe { event.as_ref() };
        match event.downcast_ref::<MPChangePlaybackPositionCommandEvent>() {
            Some(change) => {
                #[allow(unsafe_code)]
                let position = unsafe { change.positionTime() };
                handler(Command::SeekTo(position));
                MPRemoteCommandHandlerStatus::Success
            }
            None => MPRemoteCommandHandlerStatus::CommandFailed,
        }
    });
    #[allow(unsafe_code)]
    unsafe {
        let command = center.changePlaybackPositionCommand();
        command.setEnabled(true);
        command.addTargetWithHandler(&block);
    }
}

fn publish_info(info: Option<&Info>, artwork: Option<&PreparedArtwork>) {
    #[allow(unsafe_code)]
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
    let Some(info) = info else {
        SHOWN.set(None);
        #[allow(unsafe_code)]
        unsafe {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
        }
        return;
    };

    let shown: Retained<NSMutableDictionary<NSString, AnyObject>> = NSMutableDictionary::new();
    let text = |name: &NSString, value: &str| shown.insert(name, &*NSString::from_str(value));
    // SAFETY: the keys are MediaPlayer's constant strings.
    #[allow(unsafe_code)]
    unsafe {
        text(MPMediaItemPropertyTitle, &info.title);
        if let Some(artist) = &info.artist {
            text(MPMediaItemPropertyArtist, artist);
        }
        if let Some(album) = &info.album {
            text(MPMediaItemPropertyAlbumTitle, album);
        }
        let audio = NSNumber::new_usize(MPNowPlayingInfoMediaType::Audio.0);
        shown.insert(MPNowPlayingInfoPropertyMediaType, &*audio);
        match info.duration {
            Some(seconds) => shown.insert(
                MPMediaItemPropertyPlaybackDuration,
                &*NSNumber::new_f64(seconds),
            ),
            // Radio: no progress bar.
            None => shown.insert(
                MPNowPlayingInfoPropertyIsLiveStream,
                &*NSNumber::new_bool(true),
            ),
        }
        if let Some(artwork) = artwork.and_then(media_artwork) {
            shown.insert(MPMediaItemPropertyArtwork, &*artwork);
        }
    }
    SHOWN.set(Some(shown));
    show(&center);
}

fn publish_playback(playback: Playback) {
    PLAYBACK.set(playback);
    #[allow(unsafe_code)]
    let center = unsafe { MPNowPlayingInfoCenter::defaultCenter() };
    show(&center);
}

/// Writes the playback values into the shown dictionary and hands both to the system.
fn show(center: &MPNowPlayingInfoCenter) {
    let playback = PLAYBACK.get();
    let (state, position, rate) = match playback {
        Playback::Stopped => (MPNowPlayingPlaybackState::Stopped, 0.0, 0.0),
        Playback::Paused { position } => (MPNowPlayingPlaybackState::Paused, position, 0.0),
        Playback::Playing { position, rate } => {
            (MPNowPlayingPlaybackState::Playing, position, rate)
        }
    };
    SHOWN.with_borrow(|shown| {
        let Some(shown) = shown else {
            return;
        };
        // SAFETY: MediaPlayer's keys and API on the main thread; the system copies the
        // dictionary, so it can change afterwards.
        #[allow(unsafe_code)]
        unsafe {
            shown.insert(
                MPNowPlayingInfoPropertyElapsedPlaybackTime,
                &*NSNumber::new_f64(position),
            );
            shown.insert(
                MPNowPlayingInfoPropertyPlaybackRate,
                &*NSNumber::new_f64(rate),
            );
            center.setNowPlayingInfo(Some(shown));
        }
    });
    #[allow(unsafe_code)]
    unsafe {
        center.setPlaybackState(state);
    }
}

/// The cover as the system wants it: an image it asks for at the size it needs.
fn media_artwork(artwork: &PreparedArtwork) -> Option<Retained<MPMediaItemArtwork>> {
    let data = NSData::with_bytes(&artwork.image.bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    let size: CGSize = image.size();
    if size.width <= 0.0 || size.height <= 0.0 {
        return None;
    }
    // The same image at every size: the system scales it.
    let request = RcBlock::new(move |_size: CGSize| NonNull::from(&*image));
    // SAFETY: the block returns an image it keeps alive.
    #[allow(unsafe_code)]
    let artwork = unsafe {
        MPMediaItemArtwork::initWithBoundsSize_requestHandler(
            MPMediaItemArtwork::alloc(),
            size,
            &request,
        )
    };
    Some(artwork)
}
