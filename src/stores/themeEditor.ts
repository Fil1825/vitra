import { create } from 'zustand';

import type { Theme } from '../theme/schema.ts';

/** A theme being edited: shown live in every part of the window until saved or dropped. */
export interface ThemeDraft {
  theme: Theme;
  css: string | null;
  /** The theme's asset file names (its folder exists once it was saved or had a picture added). */
  assets: readonly string[];
  /** The saved theme it started from, to tell whether anything changed. */
  saved: { theme: Theme; css: string | null } | null;
}

export const useThemeEditor = create<{ draft: ThemeDraft | null }>(() => ({ draft: null }));
