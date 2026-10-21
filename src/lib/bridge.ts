import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';

import { bus } from './bus.ts';
import { SETTINGS_CHANGED_EVENT, type Settings } from './settings.ts';
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
  ]);
  return () => unlisten.forEach((fn) => fn());
}
