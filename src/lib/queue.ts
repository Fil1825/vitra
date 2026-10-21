import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/player/queue.rs.

export type Repeat = 'off' | 'all' | 'one';

export interface QueueItem {
  /** Unique within the queue (the same track can be queued twice). */
  uid: number;
  path: string;
  /** Set for library tracks. */
  trackId: number | null;
  title: string;
  artist: string | null;
  album: string | null;
  duration: number | null;
  cover: string | null;
}

export interface QueueState {
  items: QueueItem[];
  /** Index of the playing item; earlier items were played, later ones are up next. */
  current: number | null;
  shuffle: boolean;
  repeat: Repeat;
}

export const QUEUE_CHANGED_EVENT = 'queue://changed';

export const EMPTY_QUEUE: QueueState = { items: [], current: null, shuffle: false, repeat: 'off' };

/** The repeat mode the repeat button switches to. */
export function nextRepeat(repeat: Repeat): Repeat {
  return repeat === 'off' ? 'all' : repeat === 'all' ? 'one' : 'off';
}

/** Whether Next has somewhere to go. */
export function hasNext(queue: QueueState): boolean {
  return (
    queue.current !== null && (queue.current + 1 < queue.items.length || queue.repeat !== 'off')
  );
}

const call = <T = void>(command: string, args?: Record<string, unknown>) =>
  isTauri()
    ? invoke<T>(command, args)
    : Promise.reject(new Error('Playback needs the desktop app'));

export const queue = {
  get: () => call<QueueState>('queue_get'),
  /** Replaces the queue with library tracks and plays the one at `start`. */
  play: (trackIds: number[], start = 0) => call('queue_play', { trackIds, start }),
  add: (trackIds: number[], next: boolean) => call('queue_add', { trackIds, next }),
  jump: (uid: number) => call('queue_jump', { uid }),
  remove: (uids: number[]) => call('queue_remove', { uids }),
  move: (from: number, to: number) => call('queue_move', { from, to }),
  clear: () => call('queue_clear'),
  setShuffle: (shuffle: boolean) => call('queue_set_shuffle', { shuffle }),
  setRepeat: (repeat: Repeat) => call('queue_set_repeat', { repeat }),
};
