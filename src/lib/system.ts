import { invoke, isTauri } from '@tauri-apps/api/core';

/** The words of the menu-bar (tray) icon's menu, in the app's language. */
export interface TrayLabels {
  notPlaying: string;
  play: string;
  pause: string;
  next: string;
  previous: string;
  show: string;
  miniPlayer: string;
  quit: string;
}

export const system = {
  setTrayLabels: async (labels: TrayLabels): Promise<void> => {
    if (isTauri()) await invoke('tray_labels', { labels });
  },
  /** Whether the system has a place for the icon (a Linux desktop may not). */
  trayAvailable: async (): Promise<boolean> =>
    isTauri() ? invoke<boolean>('tray_available') : false,
  /** Actions whose shortcut in every app the system refused (another app has it). */
  globalShortcutsFailed: async (): Promise<string[]> =>
    isTauri() ? invoke<string[]>('global_shortcuts_failed') : [],
};
