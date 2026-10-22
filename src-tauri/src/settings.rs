//! User settings, persisted as JSON in the app config directory.
//!
//! Rust owns the settings so every window (and later the tray and global hotkeys) sees the same
//! values. Loading is forgiving: unknown fields are ignored and missing or invalid values fall
//! back to defaults, so a file written by an older or newer version never resets everything.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use links::AudioFormat;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::AppError;
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
}

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
    /// File name template, e.g. `{artist} - {title}`.
    #[serde(deserialize_with = "or_default")]
    pub template: String,
}

impl Default for DownloadSettings {
    fn default() -> Self {
        Self {
            folder: String::new(),
            format: AudioFormat::Original,
            mp3_bitrate: 320,
            template: links::DEFAULT_TEMPLATE.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LibrarySettings {
    /// Files opened or dropped outside the library folders join the library as single
    /// tracks, so they get favorites, history and playlists too.
    #[serde(deserialize_with = "or_true")]
    pub add_opened_files: bool,
}

impl Default for LibrarySettings {
    fn default() -> Self {
        Self {
            add_opened_files: true,
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
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
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
        // Only an absolute folder makes sense; anything else means the default.
        if !Path::new(downloads.folder.trim()).is_absolute() {
            downloads.folder.clear();
        }
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
    let previous = store.get().appearance.window_material;
    let saved = store.set(settings)?;
    if saved.appearance.window_material != previous {
        window::reapply_all(&app, saved.appearance.window_material);
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
                ..Appearance::default()
            },
            library: LibrarySettings {
                add_opened_files: false,
            },
            downloads: DownloadSettings {
                folder: std::env::temp_dir()
                    .join("Vitra")
                    .to_string_lossy()
                    .into_owned(),
                format: AudioFormat::Mp3,
                mp3_bitrate: 192,
                template: "{title}".into(),
            },
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
                    "artworkAccent": true
                },
                "library": { "addOpenedFiles": true },
                "downloads": {
                    "folder": "",
                    "format": "original",
                    "mp3Bitrate": 320,
                    "template": "{artist} - {title}"
                }
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
}
