//! User settings, persisted as JSON in the app config directory.
//!
//! Rust owns the settings so every window (and later the tray and global hotkeys) sees the same
//! values. Loading is forgiving: unknown fields are ignored and missing or invalid values fall
//! back to defaults, so a file written by an older or newer version never resets everything.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use library::ArtistSplitting;
use links::AudioFormat;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::AppError;
use crate::global_shortcuts::GlobalKeys;
use crate::layout::{Layout, LayoutPreset, Module, sanitize_presets};
use crate::library::LibraryService;
use crate::lyrics::LyricsService;
use crate::mini::{MiniPlayer, Mode, SizePreset};
use crate::player::Player;
use crate::shortcuts::{self, GlobalShortcuts, Shortcuts};
use crate::sound::SoundSettings;
use crate::tray::Tray;
use crate::window;

/// Emitted to every window with the new [`Settings`] after a change.
pub const SETTINGS_CHANGED: &str = "settings://changed";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    #[serde(deserialize_with = "or_default")]
    pub language: Language,
    #[serde(deserialize_with = "or_default")]
    pub appearance: Appearance,
    #[serde(deserialize_with = "or_default")]
    pub library: LibrarySettings,
    #[serde(deserialize_with = "or_default")]
    pub downloads: DownloadSettings,
    #[serde(deserialize_with = "or_default")]
    pub lyrics: LyricsSettings,
    #[serde(deserialize_with = "or_default")]
    pub mini_player: MiniPlayerSettings,
    /// The main window's modules and where they are (see `layout.rs`).
    #[serde(deserialize_with = "or_default")]
    pub layout: Layout,
    /// Layouts saved under a name.
    #[serde(deserialize_with = "or_default")]
    pub layout_presets: Vec<LayoutPreset>,
    /// Keyboard shortcuts the user changed (see `shortcuts.rs`).
    #[serde(deserialize_with = "or_default")]
    pub shortcuts: Shortcuts,
    /// Shortcuts that work in every app: action id → combination (see `global_shortcuts.rs`).
    #[serde(deserialize_with = "or_default")]
    pub global_shortcuts: GlobalShortcuts,
    /// The level meter module.
    #[serde(deserialize_with = "or_default")]
    pub meter: MeterSettings,
    /// The equalizer, volume normalization and crossfade (see `sound.rs`).
    #[serde(deserialize_with = "or_default")]
    pub sound: SoundSettings,
    #[serde(deserialize_with = "or_default")]
    pub system: SystemSettings,
}

/// How Vitra sits in the system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SystemSettings {
    /// The icon in the menu bar (macOS) or the notification area (see `tray.rs`).
    pub tray_icon: bool,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self { tray_icon: true }
    }
}

/// The level meter module.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeterSettings {
    #[serde(deserialize_with = "or_default")]
    pub style: MeterStyle,
}

/// How the level meter draws the spectrum.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeterStyle {
    /// Bars rising from the bottom.
    #[default]
    Bars,
    /// Bars growing both ways from the middle.
    Mirror,
    /// A smooth line over the bands.
    Wave,
}

/// What mpv's audio chain measures for the windows (see `player::level`, `player::spectrum`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioMeters {
    /// The loudness, for the backdrop's pulse.
    pub pulse: bool,
    /// The spectrum, for the level meter on screen.
    pub spectrum: bool,
}

impl Settings {
    pub fn audio_meters(&self) -> AudioMeters {
        AudioMeters {
            pulse: self.appearance.backdrop_pulse,
            spectrum: self.layout.shows(Module::Level),
        }
    }
}

/// The mini player window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MiniPlayerSettings {
    #[serde(deserialize_with = "or_default")]
    pub mode: Mode,
    #[serde(deserialize_with = "or_default")]
    pub size: SizePreset,
    /// Stays above other windows.
    #[serde(deserialize_with = "or_true")]
    pub always_on_top: bool,
    /// macOS: above full-screen apps too. macOS puts an app's windows into another app's
    /// full-screen space only for apps without a Dock icon, so the icon goes away while only
    /// the mini player is open.
    #[serde(deserialize_with = "or_default")]
    pub over_full_screen: bool,
    /// How opaque the glass background is (0.2–1); artwork, text and buttons stay solid.
    #[serde(deserialize_with = "or_default")]
    pub opacity: f64,
    /// Edges dropped near the screen's edges snap to them.
    #[serde(deserialize_with = "or_true")]
    pub snap_to_edges: bool,
    /// Opens where it was, at the size each mode had.
    #[serde(deserialize_with = "or_true")]
    pub remember_position: bool,
    /// Clicks go to the windows below; holding ⌥ (Alt) over it makes it usable.
    #[serde(deserialize_with = "or_default")]
    pub click_through: bool,
    /// Opening the mini player hides the main window, as Apple Music's MiniPlayer does.
    #[serde(deserialize_with = "or_true")]
    pub hide_main_window: bool,
}

impl Default for MiniPlayerSettings {
    fn default() -> Self {
        Self {
            mode: Mode::Compact,
            size: SizePreset::Medium,
            always_on_top: true,
            over_full_screen: false,
            opacity: 1.0,
            snap_to_edges: true,
            remember_position: true,
            click_through: false,
            hide_main_window: true,
        }
    }
}

/// Records' speeds the user can choose, in turns a minute (a slow crawl up to a 78).
pub const VINYL_SPEED: std::ops::RangeInclusive<f64> = 5.0..=78.0;

/// The range of the mini player's background opacity: below it the glass is gone.
pub const MINI_OPACITY: std::ops::RangeInclusive<f64> = 0.2..=1.0;

/// The lyrics view and where lyrics come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LyricsSettings {
    /// Look lyrics up on LRCLIB when a track has none (sends its title, artist, album and
    /// duration to lrclib.net).
    #[serde(deserialize_with = "or_default")]
    pub online: bool,
    /// Text size in the lyrics view, relative to the theme's.
    #[serde(deserialize_with = "or_default")]
    pub font_scale: f64,
    /// Show a translation line under the original when the lyrics have one.
    #[serde(deserialize_with = "or_true")]
    pub show_translation: bool,
    /// Show lyrics at all: off hides them in Now Playing, the side panel and the mini
    /// player's lyrics mode until they're asked for again.
    #[serde(deserialize_with = "or_true")]
    pub visible: bool,
}

impl Default for LyricsSettings {
    fn default() -> Self {
        Self {
            online: false,
            font_scale: 1.0,
            show_translation: true,
            visible: true,
        }
    }
}

/// The range the lyrics view's text size can be scaled in.
const FONT_SCALES: std::ops::RangeInclusive<f64> = 0.7..=1.6;

/// Downloading links into the library.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadSettings {
    /// Where downloads go; empty means `<Music>/Vitra`.
    #[serde(deserialize_with = "or_default")]
    pub folder: String,
    #[serde(deserialize_with = "or_default")]
    pub format: AudioFormat,
    /// kbit/s, for MP3 (one of [`links::MP3_BITRATES`]).
    #[serde(deserialize_with = "or_default")]
    pub mp3_bitrate: u32,
    /// File name template, e.g. `{artist} - {title}`; `/` makes folders.
    #[serde(deserialize_with = "or_default")]
    pub template: String,
    /// The template for tracks of an album, e.g. `{album_artist}/{album}/{track} {title}`.
    #[serde(deserialize_with = "or_default")]
    pub album_template: String,
    /// Browser whose cookies yt-dlp uses (one of [`links::COOKIE_BROWSERS`]); empty for
    /// none. Signed in there, age-restricted videos play.
    #[serde(deserialize_with = "or_default")]
    pub cookies_from_browser: String,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            folder: String::new(),
            format: AudioFormat::Original,
            mp3_bitrate: 320,
            template: links::DEFAULT_TEMPLATE.to_owned(),
            album_template: links::DEFAULT_ALBUM_TEMPLATE.to_owned(),
            cookies_from_browser: String::new(),
        }
    }
}

impl DownloadSettings {
    /// The browser to take cookies from, if one is set.
    pub fn cookies_browser(&self) -> Option<String> {
        let browser = self.cookies_from_browser.trim();
        (!browser.is_empty()).then(|| browser.to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibrarySettings {
    /// Files opened or dropped outside the library folders join the library as single
    /// tracks, so they get favorites, history and playlists too.
    #[serde(deserialize_with = "or_true")]
    pub add_opened_files: bool,
    /// How a credit like "A feat. B" or "A, B" is split into artists. The library keeps a
    /// copy, so its scanner splits the same way.
    #[serde(deserialize_with = "or_default")]
    pub artist_splitting: ArtistSplitting,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            add_opened_files: true,
            artist_splitting: ArtistSplitting::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    En,
    Ru,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Appearance {
    /// `auto` or a theme id; themes themselves live in the frontend.
    #[serde(deserialize_with = "or_default")]
    pub theme: String,
    #[serde(deserialize_with = "or_default")]
    pub motion: Preference,
    #[serde(deserialize_with = "or_default")]
    pub transparency: Preference,
    #[serde(deserialize_with = "or_default")]
    pub refraction: Switch,
    #[serde(deserialize_with = "or_default")]
    pub power_saving: bool,
    /// System material behind the window on macOS.
    #[serde(deserialize_with = "or_default")]
    pub window_material: WindowMaterial,
    /// Tint the accent color from the playing track's artwork (themes can still pin it).
    #[serde(deserialize_with = "or_true")]
    pub artwork_accent: bool,
    /// The animated backdrop swells with the music (its loudness, a kick on beats).
    #[serde(deserialize_with = "or_true")]
    pub backdrop_pulse: bool,
    /// Covers as spinning records: as the theme says (`auto`), or always / never.
    #[serde(deserialize_with = "or_default")]
    pub vinyl: Switch,
    /// How fast records turn, in turns a minute; `None` (the default) is the theme's speed,
    /// else 33⅓. Settings files from before it simply lack it.
    #[serde(deserialize_with = "or_default")]
    pub vinyl_speed: Option<f64>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: AUTO_THEME.to_owned(),
            motion: Preference::System,
            transparency: Preference::System,
            refraction: Switch::Auto,
            power_saving: false,
            window_material: WindowMaterial::Vibrancy,
            artwork_accent: true,
            backdrop_pulse: true,
            vinyl: Switch::Auto,
            vinyl_speed: None,
        }
    }
}

const AUTO_THEME: &str = "auto";

/// `LiquidGlass` (macOS 26+) falls back to `Vibrancy` on older systems.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowMaterial {
    #[default]
    Vibrancy,
    LiquidGlass,
}

/// Follow the OS setting, or force reduced / full effects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preference {
    #[default]
    System,
    Reduce,
    Full,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    #[default]
    Auto,
    On,
    Off,
}

/// Deserializes a field, falling back to its default when the value has the wrong shape.
pub(crate) fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

/// Like [`or_default`] for flags that default to `true`.
fn or_true<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or(true))
}

impl Settings {
    /// Normalizes values serde cannot check on its own.
    fn sanitized(mut self) -> Self {
        let theme = &self.appearance.theme;
        let valid = !theme.is_empty()
            && theme.len() <= 64
            && theme
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !valid {
            self.appearance.theme = AUTO_THEME.to_owned();
        }
        let downloads = &mut self.downloads;
        if !links::MP3_BITRATES.contains(&downloads.mp3_bitrate) {
            downloads.mp3_bitrate = DownloadSettings::default().mp3_bitrate;
        }
        if downloads.template.trim().is_empty() {
            downloads.template = links::DEFAULT_TEMPLATE.to_owned();
        }
        if downloads.album_template.trim().is_empty() {
            downloads.album_template = links::DEFAULT_ALBUM_TEMPLATE.to_owned();
        }
        let browser = downloads.cookies_from_browser.trim().to_ascii_lowercase();
        downloads.cookies_from_browser = if links::COOKIE_BROWSERS.contains(&browser.as_str()) {
            browser
        } else {
            String::new()
        };
        let splitting = &mut self.library.artist_splitting;
        let mut exceptions: Vec<String> = Vec::new();
        for name in &splitting.exceptions {
            let name = name.trim();
            if !name.is_empty() && !exceptions.iter().any(|known| known == name) {
                exceptions.push(name.to_owned());
            }
        }
        splitting.exceptions = exceptions;
        // Only an absolute folder makes sense; anything else means the default.
        if !Path::new(downloads.folder.trim()).is_absolute() {
            downloads.folder.clear();
        }
        self.layout_presets = sanitize_presets(std::mem::take(&mut self.layout_presets));
        self.shortcuts = shortcuts::sanitize(std::mem::take(&mut self.shortcuts));
        self.global_shortcuts =
            shortcuts::sanitize_global(std::mem::take(&mut self.global_shortcuts));
        self.sound = std::mem::take(&mut self.sound).sanitized();
        let scale = &mut self.lyrics.font_scale;
        *scale = if scale.is_finite() && *scale > 0.0 {
            scale.clamp(*FONT_SCALES.start(), *FONT_SCALES.end())
        } else {
            1.0
        };
        self.appearance.vinyl_speed = self
            .appearance
            .vinyl_speed
            .filter(|speed| speed.is_finite())
            .map(|speed| speed.clamp(*VINYL_SPEED.start(), *VINYL_SPEED.end()));
        let opacity = &mut self.mini_player.opacity;
        *opacity = if opacity.is_finite() && *opacity > 0.0 {
            opacity.clamp(*MINI_OPACITY.start(), *MINI_OPACITY.end())
        } else {
            1.0
        };
        self
    }
}

/// The current settings plus the file they are saved to.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> Self {
        let current = Mutex::new(read(&path));
        Self { path, current }
    }

    pub fn get(&self) -> Settings {
        self.lock().clone()
    }

    pub fn set(&self, settings: Settings) -> Result<Settings, AppError> {
        let settings = settings.sanitized();
        // Hold the lock while writing so concurrent updates are saved in order.
        let mut current = self.lock();
        write_atomic(&self.path, &settings)?;
        *current = settings.clone();
        Ok(settings)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Settings> {
        self.current
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn read(path: &Path) -> Settings {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == ErrorKind::NotFound => return Settings::default(),
        Err(err) => {
            log::warn!("cannot read {}: {err}; using defaults", path.display());
            return Settings::default();
        }
    };
    match serde_json::from_str::<Settings>(&text) {
        Ok(settings) => settings.sanitized(),
        Err(err) => {
            // Keep the broken file for the user instead of silently overwriting it later.
            let backup = path.with_extension("json.bak");
            log::warn!(
                "invalid settings file {}: {err}; backed up to {}",
                path.display(),
                backup.display()
            );
            if let Err(err) = fs::rename(path, &backup) {
                log::warn!("cannot back up settings: {err}");
            }
            Settings::default()
        }
    }
}

/// Writes through a temporary file so a crash never leaves a half-written settings file.
fn write_atomic(path: &Path, settings: &Settings) -> Result<(), AppError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(settings)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[tauri::command]
pub fn get_settings(store: State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// Saves the settings and broadcasts them to all windows; returns what was actually stored.
#[tauri::command]
pub fn set_settings(
    app: AppHandle,
    store: State<'_, SettingsStore>,
    settings: Settings,
) -> Result<Settings, AppError> {
    let previous = store.get();
    let saved = store.set(settings)?;
    if saved.appearance.window_material != previous.appearance.window_material {
        window::reapply_all(&app);
    }
    if saved.library.artist_splitting != previous.library.artist_splitting
        && let Some(library) = app.try_state::<LibraryService>()
    {
        library.set_artist_splitting(saved.library.artist_splitting.clone());
    }
    if saved.downloads.cookies_from_browser != previous.downloads.cookies_from_browser
        && let Some(player) = app.try_state::<Player>()
    {
        player.links_settings_changed();
    }
    // A search stopped for want of readable cookies may go on with another browser.
    if saved.downloads.cookies_from_browser != previous.downloads.cookies_from_browser
        && let Some(import) = app.try_state::<crate::import::ImportService>()
    {
        import.resume();
    }
    if saved.audio_meters() != previous.audio_meters()
        && let Some(player) = app.try_state::<Player>()
    {
        player.meter_filters_changed();
    }
    if saved.sound != previous.sound
        && let Some(player) = app.try_state::<Player>()
    {
        player.sound_changed(&previous.sound, &saved.sound);
    }
    if saved.system.tray_icon != previous.system.tray_icon
        && let Some(tray) = app.try_state::<Tray>()
    {
        tray.set_visible(saved.system.tray_icon);
    }
    if saved.global_shortcuts != previous.global_shortcuts
        && let Some(keys) = app.try_state::<GlobalKeys>()
    {
        keys.apply(&app, &saved.global_shortcuts);
    }
    if saved.mini_player != previous.mini_player
        && let Some(mini) = app.try_state::<MiniPlayer>()
    {
        mini.settings_changed(&previous.mini_player, &saved.mini_player);
    }
    // Turning online lyrics on looks the playing track up right away.
    if saved.lyrics.online
        && !previous.lyrics.online
        && let Some(lyrics) = app.try_state::<LyricsService>()
    {
        lyrics.reload();
    }
    app.emit(SETTINGS_CHANGED, &saved)?;
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &Path) -> SettingsStore {
        SettingsStore::load(dir.join("settings.json"))
    }

    #[test]
    fn missing_file_gives_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(store_in(tmp.path()).get(), Settings::default());
    }

    #[test]
    fn saved_settings_survive_a_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            language: Language::Ru,
            appearance: Appearance {
                theme: "aurora-night".into(),
                power_saving: true,
                vinyl: Switch::On,
                vinyl_speed: Some(45.0),
                ..Appearance::default()
            },
            library: LibrarySettings {
                add_opened_files: false,
                artist_splitting: ArtistSplitting {
                    enabled: false,
                    exceptions: vec!["Foo & Bar".into()],
                },
            },
            downloads: DownloadSettings {
                folder: std::env::temp_dir()
                    .join("Vitra")
                    .to_string_lossy()
                    .into_owned(),
                format: AudioFormat::Mp3,
                mp3_bitrate: 192,
                template: "{title}".into(),
                album_template: "{album}/{track} {title}".into(),
                cookies_from_browser: "firefox".into(),
            },
            lyrics: LyricsSettings {
                online: true,
                font_scale: 1.2,
                show_translation: false,
                visible: false,
            },
            mini_player: MiniPlayerSettings {
                mode: Mode::Lyrics,
                size: SizePreset::Large,
                always_on_top: false,
                over_full_screen: true,
                opacity: 0.5,
                snap_to_edges: false,
                remember_position: false,
                click_through: true,
                hide_main_window: false,
            },
            layout: serde_json::from_value(serde_json::json!({
                "zones": { "right": ["sidebar"], "left": ["queue"], "top": ["controls"] },
                "hidden": [],
                "widths": { "left": 300, "right": 250 },
                "weights": { "queue": 2 },
                "sidebar": "rail"
            }))
            .unwrap(),
            layout_presets: vec![LayoutPreset {
                name: "Focus".into(),
                layout: Layout::default(),
            }],
            shortcuts: Shortcuts::from([
                ("mute".to_owned(), None),
                ("play-pause".to_owned(), Some("KeyK".to_owned())),
            ]),
            global_shortcuts: GlobalShortcuts::from([(
                "play-pause".to_owned(),
                "Mod+Alt+KeyP".to_owned(),
            )]),
            meter: MeterSettings {
                style: MeterStyle::Mirror,
            },
            sound: SoundSettings {
                equalizer: crate::sound::Equalizer {
                    enabled: true,
                    preset: "rock".into(),
                    gains: vec![4.5, 3.0, -1.5, -2.0, -1.0, 1.0, 2.5, 3.5, 4.0, 4.0],
                    preamp: -4.5,
                },
                presets: vec![crate::sound::EqPreset {
                    name: "Car".into(),
                    gains: vec![2.0; 10],
                    preamp: -2.0,
                }],
                normalization: crate::sound::Normalization::Album,
                crossfade: 6.0,
            },
            system: SystemSettings { tray_icon: false },
        };

        store_in(tmp.path()).set(settings.clone()).unwrap();

        assert_eq!(store_in(tmp.path()).get(), settings);
    }

    #[test]
    fn serializes_as_camel_case_json() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "language": "system",
                "appearance": {
                    "theme": "auto",
                    "motion": "system",
                    "transparency": "system",
                    "refraction": "auto",
                    "powerSaving": false,
                    "windowMaterial": "vibrancy",
                    "artworkAccent": true,
                    "backdropPulse": true,
                    "vinyl": "auto",
                    "vinylSpeed": null
                },
                "library": {
                    "addOpenedFiles": true,
                    "artistSplitting": { "enabled": true, "exceptions": [] }
                },
                "downloads": {
                    "folder": "",
                    "format": "original",
                    "mp3Bitrate": 320,
                    "template": "{artist} - {title}",
                    "albumTemplate": "{album_artist}/{album}/{track} {title}",
                    "cookiesFromBrowser": ""
                },
                "lyrics": {
                    "online": false,
                    "fontScale": 1.0,
                    "showTranslation": true,
                    "visible": true
                },
                "miniPlayer": {
                    "mode": "compact",
                    "size": "medium",
                    "alwaysOnTop": true,
                    "overFullScreen": false,
                    "opacity": 1.0,
                    "snapToEdges": true,
                    "rememberPosition": true,
                    "clickThrough": false,
                    "hideMainWindow": true
                },
                "layout": {
                    "zones": {
                        "left": ["sidebar"],
                        "right": ["queue", "lyrics", "artwork"],
                        "top": [],
                        "bottom": ["controls", "level"]
                    },
                    "hidden": ["queue", "lyrics", "artwork", "level"],
                    "widths": { "left": 232.0, "right": 320.0 },
                    "weights": {},
                    "sidebar": "expanded"
                },
                "layoutPresets": [],
                "shortcuts": {},
                "globalShortcuts": {},
                "meter": { "style": "bars" },
                "sound": {
                    "equalizer": {
                        "enabled": false,
                        "preset": "flat",
                        "gains": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                        "preamp": 0.0
                    },
                    "presets": [],
                    "normalization": "off",
                    "crossfade": 0.0
                },
                "system": { "trayIcon": true }
            })
        );
    }

    #[test]
    fn invalid_fields_fall_back_individually() {
        let settings: Settings = serde_json::from_str(
            r#"{
                "language": "klingon",
                "appearance": {
                    "theme": "dark",
                    "motion": 42,
                    "powerSaving": true,
                    "windowMaterial": "chrome"
                },
                "fromTheFuture": { "x": 1 }
            }"#,
        )
        .unwrap();

        assert_eq!(settings.language, Language::System);
        assert_eq!(settings.appearance.theme, "dark");
        assert_eq!(settings.appearance.motion, Preference::System);
        assert!(settings.appearance.power_saving);
        assert_eq!(
            settings.appearance.window_material,
            WindowMaterial::Vibrancy
        );
    }

    #[test]
    fn adding_opened_files_defaults_to_on() {
        let old: Settings = serde_json::from_str(r#"{ "language": "en" }"#).unwrap();
        assert!(old.library.add_opened_files);
        let off: Settings =
            serde_json::from_str(r#"{ "library": { "addOpenedFiles": false } }"#).unwrap();
        assert!(!off.library.add_opened_files);
        let broken: Settings =
            serde_json::from_str(r#"{ "library": { "addOpenedFiles": "no" } }"#).unwrap();
        assert!(broken.library.add_opened_files);
    }

    #[test]
    fn odd_download_settings_fall_back() {
        let settings: Settings = serde_json::from_str(
            r#"{ "downloads": {
                "folder": "relative/dir", "format": "wav", "mp3Bitrate": 1000, "template": " "
            } }"#,
        )
        .unwrap();
        assert_eq!(settings.sanitized().downloads, DownloadSettings::default());
    }

    #[test]
    fn lyrics_text_size_stays_in_range() {
        let read = |json: &str| -> LyricsSettings {
            serde_json::from_str::<Settings>(json)
                .unwrap()
                .sanitized()
                .lyrics
        };
        assert_eq!(read(r#"{ "lyrics": { "fontScale": 9 } }"#).font_scale, 1.6);
        assert_eq!(read(r#"{ "lyrics": { "fontScale": 0 } }"#).font_scale, 1.0);
        assert_eq!(
            read(r#"{ "lyrics": { "fontScale": "big" } }"#).font_scale,
            1.0
        );
        let old = read(r#"{ "language": "en" }"#);
        assert_eq!(old, LyricsSettings::default());
    }

    #[test]
    fn record_speed_is_optional_and_stays_in_range() {
        let read = |json: &str| -> Option<f64> {
            serde_json::from_str::<Settings>(json)
                .unwrap()
                .sanitized()
                .appearance
                .vinyl_speed
        };
        // Settings saved before the speed existed: the theme's speed, else 33⅓.
        assert_eq!(read(r#"{ "appearance": { "vinyl": "on" } }"#), None);
        assert_eq!(read(r#"{ "appearance": { "vinylSpeed": null } }"#), None);
        assert_eq!(
            read(r#"{ "appearance": { "vinylSpeed": 45 } }"#),
            Some(45.0)
        );
        assert_eq!(
            read(r#"{ "appearance": { "vinylSpeed": 300 } }"#),
            Some(78.0)
        );
        assert_eq!(read(r#"{ "appearance": { "vinylSpeed": 1 } }"#), Some(5.0));
        assert_eq!(read(r#"{ "appearance": { "vinylSpeed": "fast" } }"#), None);
    }

    #[test]
    fn odd_cookie_browsers_and_exceptions_are_cleaned_up() {
        let settings: Settings = serde_json::from_str(
            r#"{ "downloads": { "cookiesFromBrowser": " Safari " },
                 "library": { "artistSplitting": { "exceptions": [" A, B ", "", "A, B"] } } }"#,
        )
        .unwrap();
        let settings = settings.sanitized();
        assert_eq!(
            settings.downloads.cookies_browser().as_deref(),
            Some("safari")
        );
        assert_eq!(settings.library.artist_splitting.exceptions, ["A, B"]);
        assert!(settings.library.artist_splitting.enabled);
        let odd: Settings =
            serde_json::from_str(r#"{ "downloads": { "cookiesFromBrowser": "netscape; rm" } }"#)
                .unwrap();
        assert_eq!(odd.sanitized().downloads.cookies_browser(), None);
    }

    #[test]
    fn mini_player_settings_fall_back_and_stay_in_range() {
        let read = |json: &str| -> MiniPlayerSettings {
            serde_json::from_str::<Settings>(json)
                .unwrap()
                .sanitized()
                .mini_player
        };
        assert_eq!(
            read(r#"{ "language": "en" }"#),
            MiniPlayerSettings::default()
        );
        let odd = read(
            r#"{ "miniPlayer": { "mode": "huge", "size": 3, "opacity": 0.05, "alwaysOnTop": "x" } }"#,
        );
        assert_eq!(odd.mode, Mode::Compact);
        assert_eq!(odd.size, SizePreset::Medium);
        assert_eq!(odd.opacity, 0.2);
        assert!(odd.always_on_top);
        assert_eq!(
            read(r#"{ "miniPlayer": { "opacity": "half" } }"#).opacity,
            1.0
        );
        assert_eq!(
            read(r#"{ "miniPlayer": { "mode": "slim" } }"#).mode,
            Mode::Slim
        );
    }

    #[test]
    fn reads_liquid_glass_material() {
        let settings: Settings =
            serde_json::from_str(r#"{ "appearance": { "windowMaterial": "liquidGlass" } }"#)
                .unwrap();
        assert_eq!(
            settings.appearance.window_material,
            WindowMaterial::LiquidGlass
        );
    }

    #[test]
    fn invalid_theme_id_becomes_auto() {
        let tmp = tempfile::tempdir().unwrap();
        let settings = Settings {
            appearance: Appearance {
                theme: "../../evil".into(),
                ..Appearance::default()
            },
            ..Settings::default()
        };

        let saved = store_in(tmp.path()).set(settings).unwrap();

        assert_eq!(saved.appearance.theme, "auto");
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_are_used() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        fs::write(&path, "{ not json").unwrap();

        assert_eq!(store_in(tmp.path()).get(), Settings::default());
        assert!(tmp.path().join("settings.json.bak").exists());
        assert!(!path.exists());
    }

    #[test]
    fn the_pulse_and_the_spectrum_are_measured_when_shown() {
        let mut settings = Settings::default();
        assert_eq!(
            settings.audio_meters(),
            AudioMeters {
                pulse: true,
                spectrum: false
            }
        );
        settings.appearance.backdrop_pulse = false;
        settings
            .layout
            .hidden
            .retain(|module| *module != Module::Level);
        assert_eq!(
            settings.audio_meters(),
            AudioMeters {
                pulse: false,
                spectrum: true
            }
        );
    }

    #[test]
    fn a_broken_layout_still_gives_every_module() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "layout": { "zones": "nope" }, "layoutPresets": [{ "name": " ", "layout": {} }] }"#,
        )
        .unwrap();
        let settings = store_in(tmp.path()).get();
        assert_eq!(settings.layout, Layout::default());
        assert!(settings.layout_presets.is_empty());
    }

    #[test]
    fn broken_shortcuts_are_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "shortcuts": { "mute": 5, "next": "Mod+KeyN" } }"#,
        )
        .unwrap();
        // A value of the wrong type spoils the whole map for serde: none of it is used.
        assert!(store_in(tmp.path()).get().shortcuts.is_empty());
        let store = store_in(tmp.path());
        let mut settings = store.get();
        settings.shortcuts = Shortcuts::from([
            ("next".to_owned(), Some("Mod+KeyN".to_owned())),
            ("Bad Id".to_owned(), None),
        ]);
        let saved = store.set(settings).unwrap();
        assert_eq!(
            saved.shortcuts,
            Shortcuts::from([("next".to_owned(), Some("Mod+KeyN".to_owned()))])
        );
    }
}
