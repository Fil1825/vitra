import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/player/state.rs and crates/media.

export type PlaybackStatus = 'idle' | 'loading' | 'playing' | 'paused';

export interface TrackMetadata {
  title: string | null;
  artist: string | null;
  album: string | null;
  albumArtist: string | null;
  genre: string | null;
  year: number | null;
  trackNumber: number | null;
  discNumber: number | null;
  duration: number | null;
  format: string | null;
  sampleRate: number | null;
  bitDepth: number | null;
  bitrate: number | null;
  channels: number | null;
}

/** Dominant artwork colors (`#RRGGBB`), most prominent first, plus the best accent. */
export interface Palette {
  colors: string[];
  accent: string;
}

export interface Track {
  id: number;
  path: string;
  /** The library track, if the file is in the library. */
  libraryId: number | null;
  /** Tag title, or the file name. */
  title: string;
  metadata: TrackMetadata;
  artwork: { id: string; palette: Palette | null } | null;
}

export type PlayerError =
  | { kind: 'mpvMissing' }
  | { kind: 'mpvFailed'; message: string }
  | { kind: 'playback'; message: string };

export interface PlayerState {
  status: PlaybackStatus;
  track: Track | null;
  /** Seconds. */
  position: number;
  duration: number | null;
  /** 0–100. */
  volume: number;
  muted: boolean;
  speed: number;
  error: PlayerError | null;
}

export interface PositionUpdate {
  position: number;
  duration: number | null;
}

export const PLAYER_STATE_EVENT = 'player://state';
export const PLAYER_POSITION_EVENT = 'player://position';
export const PLAYER_TRACK_EVENT = 'player://track-changed';

export const INITIAL_PLAYER_STATE: PlayerState = {
  status: 'idle',
  track: null,
  position: 0,
  duration: null,
  volume: 80,
  muted: false,
  speed: 1,
  error: null,
};

/** File extensions offered in the open dialog and accepted on drop (mpv plays far more). */
export const AUDIO_EXTENSIONS = [
  'mp3',
  'flac',
  'm4a',
  'aac',
  'alac',
  'ogg',
  'oga',
  'opus',
  'wav',
  'aif',
  'aiff',
  'ape',
  'wv',
  'wma',
  'dsf',
  'dff',
  'mka',
  'webm',
  'mp4',
  'mpc',
  'tta',
  'caf',
];

export function isAudioFile(path: string): boolean {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return AUDIO_EXTENSIONS.includes(ext);
}

/** URL of artwork served by the backend's `artwork://` scheme. */
export function artworkUrl(id: string, size: 'full' | 'thumb' = 'full'): string {
  return convertFileSrc(size === 'thumb' ? `${id}-thumb` : id, 'artwork');
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri()
    ? invoke<T>(command, args)
    : Promise.reject(new Error('Playback needs the desktop app'));

export const player = {
  state: () => call<PlayerState>('player_state'),
  /** Plays files and folders, replacing the queue. */
  open: (paths: string[]) => call('player_open', { paths }),
  play: () => call('player_play'),
  pause: () => call('player_pause'),
  toggle: () => call('player_toggle'),
  next: () => call('player_next'),
  /** Restarts the track, or goes back when near its start. */
  previous: () => call('player_previous'),
  seek: (position: number) => call('player_seek', { position }),
  setVolume: (volume: number) => call('player_set_volume', { volume }),
  setMuted: (muted: boolean) => call('player_set_muted', { muted }),
  setSpeed: (speed: number) => call('player_set_speed', { speed }),
};
