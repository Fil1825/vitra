import { invoke, isTauri } from '@tauri-apps/api/core';

// Mirrors src-tauri/src/mini.

export type MiniMode = 'compact' | 'slim' | 'lyrics';
export type MiniSize = 'small' | 'medium' | 'large';

export const MINI_MODES: readonly MiniMode[] = ['compact', 'slim', 'lyrics'];
export const MINI_SIZES: readonly MiniSize[] = ['small', 'medium', 'large'];

/**
 * Sent to the mini player while click-through is on: whether it takes clicks right now
 * (⌥ / Alt held with the pointer over it).
 */
export const MINI_INTERACTIVE_EVENT = 'mini://interactive';

const call = (command: string) =>
  isTauri() ? invoke<void>(command) : Promise.reject(new Error('The mini player needs the app'));

export const mini = {
  show: () => call('mini_show'),
  hide: () => call('mini_hide'),
  /** ⌥⌘M / Ctrl+Alt+M: the mini player in place of the main window, or back. */
  toggle: () => call('mini_toggle'),
  /** Back to the main window from the mini player. */
  expand: () => call('mini_expand'),
  /** The mini player has no menu bar or taskbar entry to quit from. */
  quit: () => call('app_quit'),
};
