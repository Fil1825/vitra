import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import type { Appearance } from '../theme/schema.ts';

/** System backdrop behind the transparent window; mirrors `Vibrancy` in src-tauri/src/window.rs. */
export type Vibrancy = 'macos' | 'mica' | 'acrylic' | 'none';

export interface WindowEffects {
  /** `macos`, `windows`, `linux`, or `web` outside Tauri. Used for chrome layout only. */
  platform: string;
  vibrancy: Vibrancy;
}

export async function getWindowEffects(): Promise<WindowEffects> {
  return isTauri()
    ? invoke<WindowEffects>('window_effects')
    : { platform: 'web', vibrancy: 'none' };
}

/**
 * Forces the native window appearance (title bar, vibrancy material) to match the theme;
 * `null` follows the system.
 */
export async function setWindowAppearance(appearance: Appearance | null): Promise<void> {
  if (isTauri()) {
    await getCurrentWindow().setTheme(appearance);
  }
}
