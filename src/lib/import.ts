import { invoke, isTauri } from '@tauri-apps/api/core';

import type { LibraryTrack, Playlist } from './library.ts';

// Mirrors src-tauri/src/import, crates/import and crates/library/src/imports.rs.

/** A track as the imported list describes it. */
export interface ImportedTrack {
  title: string;
  /** Main artist first. */
  artists: string[];
  album: string | null;
  /** Seconds. */
  duration: number | null;
  isrc: string | null;
  /** The track's own page on its service. */
  url: string | null;
  /** A local file the list points at. */
  path: string | null;
  swappable: boolean;
}

/**
 * How the search for an imported track went: `pending` (not looked for yet), `sure`,
 * `doubtful` (shown with a "?"), `missing` (nothing fitting), `picked` (the user chose).
 */
export type EntryState = 'pending' | 'sure' | 'doubtful' | 'missing' | 'picked';

/** A track that might be the imported one: in the library, or a YouTube Music link. */
export interface EntryCandidate {
  trackId: number | null;
  url: string | null;
  title: string;
  artists: string[];
  album: string | null;
  duration: number | null;
  /** 0–1. */
  score: number;
}

export interface ImportEntry {
  id: number;
  playlistId: number;
  source: ImportedTrack;
  state: EntryState;
  /** Best first; the first is the one chosen. */
  candidates: EntryCandidate[];
  trackId: number | null;
}

/** One place of a playlist: a track, an imported entry, or an entry with its track. */
export interface PlaylistPlace {
  position: number;
  track: LibraryTrack | null;
  entry: ImportEntry | null;
}

export interface ImportProgress {
  total: number;
  found: number;
  doubtful: number;
  missing: number;
  pending: number;
}

export interface ImportProgressUpdate extends ImportProgress {
  playlistId: number;
}

export interface ImportResult {
  playlist: Playlist;
  total: number;
  /** Tracks the library had. */
  inLibrary: number;
  /** The service gave only part of a longer list. */
  partial: boolean;
}

/** A playlist of the Music app (macOS). */
export interface MusicPlaylist {
  id: string;
  name: string;
  folder: string | null;
  tracks: number;
  smart: boolean;
}

export const IMPORT_PROGRESS_EVENT = 'import://progress';

/** Playlist files the import reads (the Music app's XML export included). */
export const PLAYLIST_FILE_EXTENSIONS = ['m3u', 'm3u8', 'xspf', 'xml', 'csv', 'tsv', 'txt'];

export function isPlaylistFile(path: string): boolean {
  const extension = path.split('.').pop()?.toLowerCase() ?? '';
  return path.includes('.') && PLAYLIST_FILE_EXTENSIONS.includes(extension);
}

const SERVICE_HOSTS = [
  /(^|\.)spotify\.com$/,
  /^spotify\.link$/,
  /^spoti\.fi$/,
  /^(music|itunes)\.apple\.com$/,
  /(^|\.)deezer\.com$/,
  /^(deezer|dzr)\.page\.link$/,
  /^music\.yandex\.[a-z]+$/,
  /^(m\.)?vk\.(com|ru)$/,
];

/**
 * Whether a link points at a music service whose lists are imported (Spotify, Apple Music,
 * Deezer, Yandex Music, VK) rather than played (YouTube & co. go to the link dialog).
 */
export function isServiceLink(text: string): boolean {
  const value = text.trim();
  if (/^spotify:(playlist|album|track):/i.test(value)) {
    return true;
  }
  try {
    const url = new URL(value);
    const host = url.hostname.toLowerCase().replace(/^www\./, '');
    return /^https?:$/.test(url.protocol) && SERVICE_HOSTS.some((pattern) => pattern.test(host));
  } catch {
    return false;
  }
}

/**
 * Whether pasted text is a list of tracks to import: several lines, most of them neither
 * empty nor too long to be a track.
 */
export function looksLikeTrackList(text: string): boolean {
  const lines = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line !== '');
  return lines.length >= 2 && lines.filter((line) => line.length <= 300).length >= 2;
}

/** The source a playlist imported from the Music app keeps. */
export function isMusicAppSource(url: string): boolean {
  return url.startsWith('music-app:');
}

/** Whether "Refresh" can read a playlist's source again (any imported list can). */
export function canRefresh(url: string | null): url is string {
  return url !== null && (isMusicAppSource(url) || /^https?:\/\//i.test(url));
}

/** "Artist — Title" of an imported track. */
export function displayName(track: Pick<ImportedTrack, 'title' | 'artists'>): string {
  const [artist] = track.artists;
  return artist ? `${artist} — ${track.title}` : track.title;
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('Import needs the desktop app'));

export const importer = {
  /**
   * Pasted text: a link, links, or tracks written out; `name` names a list that has none.
   * With `into`, the text completes that playlist (a list its page showed only part of).
   */
  text: (text: string, name: string, into: number | null = null) =>
    call<ImportResult>('import_text', { text, name, into }),
  files: (paths: string[]) => call<ImportResult[]>('import_files', { paths }),
  /** A list on a service; also brings an imported playlist up to date with its source. */
  link: (url: string) => call<ImportResult>('import_link', { url }),
  places: (playlistId: number) => call<PlaylistPlace[]>('import_places', { playlistId }),
  progress: (playlistId: number) => call<ImportProgress>('import_progress', { playlistId }),
  pick: (entryId: number, candidate: EntryCandidate) => call('import_pick', { entryId, candidate }),
  /** "Find…": the library's and YouTube Music's tracks for an entry or a typed query. */
  search: (entryId: number, query: string | null) =>
    call<EntryCandidate[]>('import_search', { entryId, query }),
  /** Looks again for the tracks that weren't found. */
  retry: (playlistId: number) => call('import_retry', { playlistId }),
  musicPlaylists: () => call<MusicPlaylist[]>('music_playlists'),
  music: (ids: string[]) => call<ImportResult[]>('import_music', { ids }),
};
