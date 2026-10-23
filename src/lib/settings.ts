import { invoke, isTauri } from '@tauri-apps/api/core';

import { DEFAULT_LAYOUT, type Layout } from '../layout/model.ts';
import type { AudioFormat } from './links.ts';
import type { MiniMode, MiniSize } from './mini.ts';

// Mirrors `Settings` in src-tauri/src/settings.rs, which owns and persists them.

export type Language = 'system' | 'en' | 'ru';
/** Follow the OS setting, or force reduced / full effects. */
export type Preference = 'system' | 'reduce' | 'full';
export type Switch = 'auto' | 'on' | 'off';
/** System material behind the window on macOS; Liquid Glass needs macOS 26. */
export type WindowMaterial = 'vibrancy' | 'liquidGlass';

export interface AppearanceSettings {
  /** `auto` or a theme id. */
  theme: string;
  motion: Preference;
  transparency: Preference;
  refraction: Switch;
  powerSaving: boolean;
  windowMaterial: WindowMaterial;
  /** Take the accent color from the playing track's artwork (a theme may pin its own). */
  artworkAccent: boolean;
  /** The animated backdrop swells with the music. */
  backdropPulse: boolean;
}

/** How a credit like "A feat. B" or "A, B" is split into artists. */
export interface ArtistSplitting {
  enabled: boolean;
  /** Names with a separator in them that are one artist ("Tyler, The Creator" is built in). */
  exceptions: string[];
}

export interface LibrarySettings {
  /** Files opened or dropped outside the library folders join the library as single tracks. */
  addOpenedFiles: boolean;
  artistSplitting: ArtistSplitting;
}

export interface DownloadSettings {
  /** Where downloads go; empty means `<Music>/Vitra`. */
  folder: string;
  format: AudioFormat;
  /** kbit/s, for MP3. */
  mp3Bitrate: number;
  /** File name template with `{artist}`, `{title}`, `{album}`, `{year}`; `/` makes folders. */
  template: string;
  /** The template for album tracks; also `{album_artist}` and `{track}`. */
  albumTemplate: string;
  /** Browser whose cookies yt-dlp uses (age-restricted videos); empty for none. */
  cookiesFromBrowser: string;
}

/** Browsers yt-dlp can take cookies from (mirrors `links::COOKIE_BROWSERS`). */
export const COOKIE_BROWSERS = [
  'safari',
  'chrome',
  'firefox',
  'edge',
  'brave',
  'chromium',
  'opera',
  'vivaldi',
] as const;

export interface LyricsSettings {
  /** Look lyrics up on LRCLIB when a track has none. */
  online: boolean;
  /** Text size in the lyrics view, relative to the theme's (0.7–1.6). */
  fontScale: number;
  /** Show a translation line under the original when the lyrics have one. */
  showTranslation: boolean;
  /** Show lyrics at all (Now Playing, the side panel, the mini player's lyrics mode). */
  visible: boolean;
}

export interface MiniPlayerSettings {
  mode: MiniMode;
  size: MiniSize;
  alwaysOnTop: boolean;
  /** macOS: above full-screen apps too (the Dock icon goes while it is the only window). */
  overFullScreen: boolean;
  /** Opacity of the glass background (0.2–1); artwork, text and buttons stay solid. */
  opacity: number;
  snapToEdges: boolean;
  /** Opens where it was, at the size each mode had. */
  rememberPosition: boolean;
  /** Clicks go through; holding ⌥ (Alt) over it makes it usable. */
  clickThrough: boolean;
  /** Opening the mini player hides the main window. */
  hideMainWindow: boolean;
}

/** A layout saved under a name. */
export interface LayoutPreset {
  name: string;
  layout: Layout;
}

export interface Settings {
  language: Language;
  appearance: AppearanceSettings;
  library: LibrarySettings;
  downloads: DownloadSettings;
  lyrics: LyricsSettings;
  miniPlayer: MiniPlayerSettings;
  /** The main window's modules and where they are (normalized by the backend). */
  layout: Layout;
  layoutPresets: LayoutPreset[];
}

/** The range of the mini player's background opacity. */
export const MINI_OPACITY_MIN = 0.2;

export const FONT_SCALE_MIN = 0.7;
export const FONT_SCALE_MAX = 1.6;

export const DEFAULT_SETTINGS: Settings = {
  language: 'system',
  appearance: {
    theme: 'auto',
    motion: 'system',
    transparency: 'system',
    refraction: 'auto',
    powerSaving: false,
    windowMaterial: 'vibrancy',
    artworkAccent: true,
    backdropPulse: true,
  },
  library: { addOpenedFiles: true, artistSplitting: { enabled: true, exceptions: [] } },
  downloads: {
    folder: '',
    format: 'original',
    mp3Bitrate: 320,
    template: '{artist} - {title}',
    albumTemplate: '{album_artist}/{album}/{track} {title}',
    cookiesFromBrowser: '',
  },
  lyrics: { online: false, fontScale: 1, showTranslation: true, visible: true },
  miniPlayer: {
    mode: 'compact',
    size: 'medium',
    alwaysOnTop: true,
    overFullScreen: false,
    opacity: 1,
    snapToEdges: true,
    rememberPosition: true,
    clickThrough: false,
    hideMainWindow: true,
  },
  layout: DEFAULT_LAYOUT,
  layoutPresets: [],
};

/** Tauri event carrying the new settings after any window changed them. */
export const SETTINGS_CHANGED_EVENT = 'settings://changed';

export async function loadSettings(): Promise<Settings> {
  return isTauri() ? invoke<Settings>('get_settings') : structuredClone(DEFAULT_SETTINGS);
}

/** Persists settings; resolves with what the backend actually stored. */
export async function saveSettings(settings: Settings): Promise<Settings> {
  return isTauri() ? invoke<Settings>('set_settings', { settings }) : settings;
}
