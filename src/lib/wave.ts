import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/wave and crates/library/src/waves.rs.

export const WAVE_PROGRESS_EVENT = 'wave://progress';

/**
 * What the background analysis does: `paused` by the user, waiting while a `scanning` runs or
 * with `powerSaving` on, or unable to run (`noFfmpeg`).
 */
export type AnalysisState =
  'idle' | 'analyzing' | 'paused' | 'scanning' | 'powerSaving' | 'noFfmpeg';

/** Where the links' analysis stands: `rateLimited` and `offline` wait until `linksRetryAt`. */
export type LinksState = 'off' | 'waiting' | 'rateLimited' | 'offline' | 'noYtDlp';

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
  skipped: 0,
};

const call = <T>(command: string, args?: Record<string, unknown>): Promise<T> =>
  isTauri() ? invoke<T>(command, args) : Promise.reject(new Error('My Wave needs the desktop app'));

export const wave = {
  analysis: () => call<AnalysisStatus>('wave_analysis'),
};
