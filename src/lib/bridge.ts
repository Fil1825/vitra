import { isTauri } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { bus } from './bus.ts';
import { SETTINGS_CHANGED_EVENT, type Settings } from './settings.ts';

/** Re-publishes backend events on the frontend bus. Returns a function that stops it. */
export async function startBackendBridge(): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => {};
  }
  const unlisten = await Promise.all([
    listen<Settings>(SETTINGS_CHANGED_EVENT, (event) =>
      bus.emit('settings:changed', event.payload),
    ),
  ]);
  return () => unlisten.forEach((fn) => fn());
}
