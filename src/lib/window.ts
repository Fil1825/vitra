import { invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

import type { Appearance } from '../theme/schema.ts';

/** System backdrop behind the transparent window; mirrors `Vibrancy` in src-tauri/src/window.rs. */
export type Vibrancy = 'macos' | 'liquidGlass' | 'mica' | 'acrylic' | 'none';

export interface WindowEffects {
  /** `macos`, `windows`, `linux`, or `web` outside Tauri. Used for chrome layout only. */
  platform: string;
  vibrancy: Vibrancy;
  /** macOS 26+ can use Liquid Glass as the window material. */
  liquidGlassSupported: boolean;
}

/** Tauri event a window receives with its new effects after the material changed. */
export const EFFECTS_CHANGED_EVENT = 'window://effects-changed';

export const NO_WINDOW_EFFECTS: WindowEffects = {
  platform: 'web',
  vibrancy: 'none',
  liquidGlassSupported: false,
};

export async function getWindowEffects(): Promise<WindowEffects> {
  return isTauri() ? invoke<WindowEffects>('window_effects') : NO_WINDOW_EFFECTS;
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
