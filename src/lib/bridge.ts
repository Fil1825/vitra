import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { bus } from './bus.ts';
import {
  LIBRARY_CHANGED_EVENT,
  LIBRARY_SCAN_EVENT,
  type LibraryChange,
  type ScanStatus,
} from './library.ts';
import { DOWNLOADS_CHANGED_EVENT, type DownloadJob } from './links.ts';
import {
  LYRICS_CHANGED_EVENT,
  LYRICS_LINE_EVENT,
  type CurrentLyrics,
  type LineUpdate,
} from './lyrics.ts';
import { SETTINGS_CHANGED_EVENT, type Settings } from './settings.ts';
import {
  PLAYER_POSITION_EVENT,
  PLAYER_STATE_EVENT,
  PLAYER_LEVEL_EVENT,
  PLAYER_TRACK_EVENT,
  type PlayerState,
  type PositionUpdate,
  type Track,
} from './player.ts';
import { QUEUE_CHANGED_EVENT, type QueueState } from './queue.ts';
import { EFFECTS_CHANGED_EVENT, type WindowEffects } from './window.ts';

/** Re-publishes backend events on the frontend bus. Returns a function that stops it. */
export async function startBackendBridge(): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => {};
  }
  const unlisten = await Promise.all([
    listen<Settings>(SETTINGS_CHANGED_EVENT, (event) =>
      bus.emit('settings:changed', event.payload),
    ),
    // Sent only to the window whose backdrop changed, so no filtering by label is needed.
    getCurrentWindow().listen<WindowEffects>(EFFECTS_CHANGED_EVENT, (event) =>
      bus.emit('window:effects', event.payload),
    ),
    listen<PlayerState>(PLAYER_STATE_EVENT, (event) => bus.emit('player:state', event.payload)),
    listen<PositionUpdate>(PLAYER_POSITION_EVENT, (event) =>
      bus.emit('player:position', event.payload),
    ),
    listen<Track>(PLAYER_TRACK_EVENT, (event) => bus.emit('player:track', event.payload)),
    listen<number>(PLAYER_LEVEL_EVENT, (event) => bus.emit('player:level', event.payload)),
    listen<QueueState>(QUEUE_CHANGED_EVENT, (event) => bus.emit('queue:changed', event.payload)),
    listen<LibraryChange>(LIBRARY_CHANGED_EVENT, (event) =>
      bus.emit('library:changed', event.payload),
    ),
    listen<ScanStatus>(LIBRARY_SCAN_EVENT, (event) => bus.emit('library:scan', event.payload)),
    listen<DownloadJob[]>(DOWNLOADS_CHANGED_EVENT, (event) =>
      bus.emit('downloads:changed', event.payload),
    ),
    listen<CurrentLyrics>(LYRICS_CHANGED_EVENT, (event) =>
      bus.emit('lyrics:changed', event.payload),
    ),
    listen<LineUpdate>(LYRICS_LINE_EVENT, (event) => bus.emit('lyrics:line', event.payload)),
  ]);
  return () => unlisten.forEach((fn) => fn());
}
