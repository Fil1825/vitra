import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/wave and crates/library/src/waves.rs.

export const WAVE_PROGRESS_EVENT = 'wave://progress';

/** The moods a wave starts from (mirrors `mood::Mode`). */
export type WaveMood = 'taste' | 'energetic' | 'calm' | 'sad';
export type WaveMode = { kind: WaveMood } | { kind: 'likeTrack'; track: number };

export const WAVE_MOODS: readonly WaveMood[] = ['taste', 'energetic', 'calm', 'sad'];

/**
 * How starting a wave went: nothing could be picked while the analysis hasn't measured a track
 * yet, or the track to start from couldn't be measured (offline, no FFmpeg).
 */
export type StartOutcome = 'started' | 'nothingMeasured' | 'trackUnmeasured';

export function sameMode(a: WaveMode | null, b: WaveMode | null): boolean {
  if (a === null || b === null) {
    return a === b;
  }
  return (
    a.kind === b.kind && (a.kind !== 'likeTrack' || (b.kind === 'likeTrack' && a.track === b.track))
  );
}

/**
 * What the background analysis does: `paused` by the user, waiting while a `scanning` runs or
 * with `powerSaving` on, or unable to run (`noFfmpeg`).
 */
export type AnalysisState =
  'idle' | 'analyzing' | 'paused' | 'scanning' | 'powerSaving' | 'noFfmpeg';

/** Where the links' analysis stands: `rateLimited` and `offline` wait until `linksRetryAt`. */
export type LinksState = 'off' | 'waiting' | 'rateLimited' | 'offline' | 'noYtDlp' | 'decodeFailed';

export interface AnalysisStatus {
  filesDone: number;
  filesTotal: number;
  linksDone: number;
  linksTotal: number;
  /** Tracks that couldn't be measured (among the done ones). */
  failed: number;
  state: AnalysisState;
  /** The track being measured. */
  current: string | null;
  links: LinksState;
  /** Unix ms. */
  linksRetryAt: number | null;
  linksProblem: 'certificates' | 'decoder' | null;
  /** Files left out this session: missing, or kept in iCloud only. */
  skipped: number;
}

export const NO_ANALYSIS: AnalysisStatus = {
  filesDone: 0,
  filesTotal: 0,
  linksDone: 0,
  linksTotal: 0,
  failed: 0,
  state: 'idle',
  current: null,
  links: 'waiting',
  linksRetryAt: null,
  linksProblem: null,
  skipped: 0,
};

const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('My Wave needs the desktop app'));

export const wave = {
  analysis: () => call<AnalysisStatus>('wave_analysis'),
  /** Replaces the queue with a wave in this mood (a wave like the playing track keeps it). */
  start: (mode: WaveMode) => call<StartOutcome>('wave_start', { mode }),
  /** "Don't like": the wave leaves the track out from now on (and skips it if it plays). */
  dislike: (trackId: number) => call<void>('wave_dislike', { trackId }),
  /** The wave stops picking; its queue stays. */
  stop: () => call<void>('wave_stop'),
};
