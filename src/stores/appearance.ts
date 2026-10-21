import { create } from 'zustand';

import { MAX_GLASS_LAYERS, type Effects } from '../theme/effects.ts';
import type { Theme } from '../theme/schema.ts';
import { BUILT_IN_THEMES } from '../theme/themes.ts';

interface AppearanceState {
  theme: Theme;
  effects: Effects;
}

/** The theme and effects in use; written by `useAppearanceController`, read by components. */
export const useAppearance = create<AppearanceState>(() => ({
  theme: BUILT_IN_THEMES[0] as Theme,
  effects: {
    motion: true,
    transparency: true,
    blur: true,
    refraction: false,
    animatedBackdrop: true,
    maxGlassLayers: MAX_GLASS_LAYERS,
  },
}));
