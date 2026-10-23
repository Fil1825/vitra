import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors crates/lyrics and src-tauri/src/lyrics.

export interface LyricsWord {
  /** Milliseconds. */
  start: number;
  /** The word with the spaces around it as written. */
  text: string;
}

export interface LyricsLine {
  /** Milliseconds; `null` in plain lyrics. */
  start: number | null;
  text: string;
  /** Word timings (enhanced LRC); their texts joined are the line's text. */
  words: LyricsWord[];
  translation: string | null;
}

export interface Lyrics {
  lines: LyricsLine[];
  synced: boolean;
  /** LRC `[offset]`: positive values show the lyrics sooner. */
  offsetMs: number;
  tags: [string, string][];
}

export type LyricsSource = 'tags' | 'file' | 'library' | 'online';
export type LyricsStatus = 'idle' | 'loading' | 'searching' | 'ready' | 'missing';

/** The playing track's lyrics. */
export interface CurrentLyrics {
  /** The player's track id these belong to. */
  trackId: number | null;
  path: string | null;
  libraryId: number | null;
  status: LyricsStatus;
  /** Timestamps are positions in the track. */
  lyrics: Lyrics | null;
  source: LyricsSource | null;
  /** The user's sync adjustment; positive shows the lyrics later. */
  offsetMs: number;
  /** Why the last online search failed. */
  error: string | null;
}

export interface LineUpdate {
  trackId: number;
  index: number | null;
}

export type SaveTarget = 'tags' | 'file' | 'library';

/** What the editor opens for a track. */
export interface EditorLyrics {
  /** LRC or plain text; empty when the track has none. */
  text: string;
  source: LyricsSource | null;
  /** Where the lyrics can be saved. */
  targets: Record<SaveTarget, boolean>;
}

export const LYRICS_CHANGED_EVENT = 'lyrics://changed';
export const LYRICS_LINE_EVENT = 'lyrics://line';

export const NO_LYRICS: CurrentLyrics = {
  trackId: null,
  path: null,
  libraryId: null,
  status: 'idle',
  lyrics: null,
  source: null,
  offsetMs: 0,
  error: null,
};

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('Lyrics need the desktop app'));

export const lyricsApi = {
  current: () => call<CurrentLyrics>('lyrics_current'),
  /** The playing track's sync adjustment in milliseconds (positive: later). */
  setOffset: (offsetMs: number) => call('lyrics_set_offset', { offsetMs }),
  /** Looks the playing track up on LRCLIB now. */
  searchOnline: () => call('lyrics_search_online'),
  get: (path: string, libraryId: number | null) =>
    call<EditorLyrics>('lyrics_get', { path, libraryId }),
  /** Parses LRC or plain text (any `[offset]` folded into the timestamps). */
  parse: (text: string) => call<Lyrics>('lyrics_parse', { text }),
  serialize: (lyrics: Lyrics) => call<string>('lyrics_serialize', { lyrics }),
  /** Saves lyrics for a track; empty text removes them from the target. */
  save: (path: string, libraryId: number | null, text: string, target: SaveTarget) =>
    call('lyrics_save', { path, libraryId, text, target }),
  import: (path: string) => call<string>('lyrics_import', { path }),
  export: (path: string, text: string) => call('lyrics_export', { path, text }),
};

/** The line playing at `positionMs`: the last one that has started (synced lyrics only). */
export function lineAt(lyrics: Lyrics, positionMs: number): number | null {
  if (!lyrics.synced) {
    return null;
  }
  let low = 0;
  let high = lyrics.lines.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    const start = lyrics.lines[middle]?.start;
    if (start !== null && start !== undefined && start <= positionMs) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }
  return low === 0 ? null : low - 1;
}

/** When a line ends: where the next one starts (or a few seconds after it, for the last). */
export function lineEnd(lyrics: Lyrics, index: number): number {
  const next = lyrics.lines[index + 1]?.start;
  const start = lyrics.lines[index]?.start ?? 0;
  return next ?? start + 5000;
}

/**
 * How far karaoke has got in a line: the index of the word being sung and how much of it
 * (0–1). Words are sung until the next word starts, the last one until the line ends.
 */
export function wordProgress(
  line: LyricsLine,
  lineEndMs: number,
  positionMs: number,
): { index: number; fraction: number } {
  const { words } = line;
  let index = -1;
  for (let i = 0; i < words.length; i++) {
    if ((words[i]?.start ?? Infinity) <= positionMs) {
      index = i;
    } else {
      break;
    }
  }
  if (index < 0) {
    return { index: -1, fraction: 0 };
  }
  const start = words[index]?.start ?? 0;
  // A word lasts until the next one, but no longer than a few seconds (a held last note).
  const end = Math.min(words[index + 1]?.start ?? lineEndMs, start + 3000);
  const fraction = end > start ? Math.min(1, (positionMs - start) / (end - start)) : 1;
  return { index, fraction };
}

/** `+0.5 s`, `−1.2 s`, `0 s`. */
export function formatOffset(offsetMs: number): string {
  const seconds = Math.abs(offsetMs) / 1000;
  const sign = offsetMs > 0 ? '+' : offsetMs < 0 ? '−' : '';
  return `${sign}${seconds.toFixed(1)} s`;
}

/** `01:23.45`, the way LRC writes times. */
export function formatStamp(ms: number): string {
  const hundredths = Math.round(ms / 10);
  const minutes = Math.floor(hundredths / 6000);
  const seconds = Math.floor((hundredths % 6000) / 100);
  const rest = hundredths % 100;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(minutes)}:${pad(seconds)}.${pad(rest)}`;
}
