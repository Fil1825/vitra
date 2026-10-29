import type { ImportProgressUpdate, MatcherStatus } from './import.ts';
import type { LibraryChange, ScanStatus } from './library.ts';
import type { DownloadJob } from './links.ts';
import type { CurrentLyrics, LineUpdate } from './lyrics.ts';
import type { Settings } from './settings.ts';
import type { PlayerState, PositionUpdate, Track } from './player.ts';
import type { QueueState } from './queue.ts';
import type { AnalysisStatus } from './wave.ts';
import type { WindowEffects } from './window.ts';
import type { Effects } from '../theme/effects.ts';
import type { Theme } from '../theme/schema.ts';

/**
 * Events inside the frontend. Backend events are re-published here by `bridge.ts`, and UI
 * modules subscribe to the bus rather than to Tauri, so a future plugin API can hook into
 * the same stream.
 */
export interface VitraEvents {
  'settings:changed': Settings;
  'theme:changed': Theme;
  /** A theme was installed, saved or removed (in any window). */
  'themes:changed': null;
  'effects:changed': Effects;
  /** The native backdrop of this window changed (e.g. Liquid Glass was switched on). */
  'window:effects': WindowEffects;
  'player:state': PlayerState;
  /** Throttled position updates between state changes. */
  'player:position': PositionUpdate;
  /** Metadata and artwork of the current track became available. */
  'player:track': Track;
  /** The music's pulse (0..1) for the animated backdrop, while one asked for it. */
  'player:level': number;
  /** The spectrum (bands low to high, 0..1; empty: nothing plays) for the level meter. */
  'player:spectrum': number[];
  'queue:changed': QueueState;
  /** Library data changed; views showing that kind of data refetch it. */
  'library:changed': LibraryChange;
  'library:scan': ScanStatus;
  /** An imported playlist's tracks were looked for (one more found, or not). */
  'import:progress': ImportProgressUpdate;
  /** The background search started on another track, paused or stopped. */
  'import:status': MatcherStatus;
  /** My Wave's background analysis moved on (a track measured, paused, waiting). */
  'wave:progress': AnalysisStatus;
  /** The download queue changed (a job was added, progressed or finished). */
  'downloads:changed': DownloadJob[];
  /** The playing track's lyrics (or their loading state, or the sync offset) changed. */
  'lyrics:changed': CurrentLyrics;
  /** Another line of the lyrics started playing. */
  'lyrics:line': LineUpdate;
  /** The mini player with click-through on takes clicks right now (⌥ / Alt held over it). */
  'mini:interactive': boolean;
}

export type Handler<T> = (payload: T) => void;

export interface Bus<E> {
  /** Subscribes to an event; returns the unsubscribe function. */
  on<K extends keyof E>(type: K, handler: Handler<E[K]>): () => void;
  /** Like `on`, but unsubscribes after the first event. */
  once<K extends keyof E>(type: K, handler: Handler<E[K]>): () => void;
  emit<K extends keyof E>(type: K, payload: E[K]): void;
}

export function createBus<E>(): Bus<E> {
  const handlers = new Map<keyof E, Set<Handler<never>>>();

  const bus: Bus<E> = {
    on(type, handler) {
      let set = handlers.get(type);
      if (!set) {
        set = new Set();
        handlers.set(type, set);
      }
      set.add(handler as Handler<never>);
      return () => {
        set.delete(handler as Handler<never>);
      };
    },
    once(type, handler) {
      const off = bus.on(type, (payload) => {
        off();
        handler(payload);
      });
      return off;
    },
    emit(type, payload) {
      // Copy so handlers may unsubscribe while the event is being delivered.
      for (const handler of [...(handlers.get(type) ?? [])]) {
        try {
          (handler as Handler<E[typeof type]>)(payload);
        } catch (err) {
          // One broken subscriber must not stop the others.
          console.error(`Handler for "${String(type)}" failed`, err);
        }
      }
    },
  };
  return bus;
}

export const bus = createBus<VitraEvents>();
