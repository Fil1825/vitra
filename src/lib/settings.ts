import { invoke, isTauri } from '@tauri-apps/api/core';

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
}

export interface LibrarySettings {
  /** Files opened or dropped outside the library folders join the library as single tracks. */
  addOpenedFiles: boolean;
}

export interface Settings {
  language: Language;
  appearance: AppearanceSettings;
  library: LibrarySettings;
}

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
  },
  library: { addOpenedFiles: true },
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
