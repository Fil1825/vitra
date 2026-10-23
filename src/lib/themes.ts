import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/themes.rs and crates/themes.

/** A theme the user installed or made, as stored (not yet checked). */
export interface InstalledTheme {
  id: string;
  /** `theme.json` as written. */
  json: string;
  css: string | null;
  /** File names in the theme's `assets/` folder. */
  assets: string[];
}

/** A `.vitratheme` file read by the backend and held until it is installed or discarded. */
export interface OpenedPackage {
  token: number;
  json: string;
  css: string | null;
  assets: string[];
}

/** Sent to every window after a theme was installed, saved or removed. */
export const THEMES_CHANGED_EVENT = 'themes://changed';

/** The package's file extension. */
export const THEME_EXTENSION = 'vitratheme';

export function isThemeFile(path: string): boolean {
  return path.toLowerCase().endsWith(`.${THEME_EXTENSION}`);
}

/** Where the page loads an installed theme's asset from (`name` without `assets/`). */
export function themeAssetUrl(id: string, name: string): string {
  // The name goes after the encoded base: convertFileSrc would encode the `/` before it.
  return `${convertFileSrc(id, 'theme')}/${encodeURIComponent(name)}`;
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('Themes need the app'));

export const themesApi = {
  list: () => (isTauri() ? invoke<InstalledTheme[]>('themes_list') : Promise.resolve([])),
  open: (path: string) => call<OpenedPackage>('theme_open', { path }),
  install: (token: number, id: string, json: string, css: string | null) =>
    call('theme_install', { token, id, json, css }),
  discard: (token: number) => call('theme_discard', { token }),
  save: (id: string, json: string, css: string | null) => call('theme_save', { id, json, css }),
  /** A copy of the installed theme `from`, with its CSS and assets, as `to`. */
  duplicate: (from: string, to: string, json: string) =>
    call('theme_duplicate', { from, to, json }),
  /** Copies a picture or font into the theme; resolves with its path there (`assets/…`). */
  addAsset: (id: string, path: string) => call<string>('theme_add_asset', { id, path }),
  /** Writes a `.vitratheme`: this JSON and CSS, plus the installed theme's assets if any. */
  export: (id: string, json: string, css: string | null, path: string) =>
    call('theme_export', { id, json, css, path }),
  remove: (id: string) => call('theme_remove', { id }),
};
