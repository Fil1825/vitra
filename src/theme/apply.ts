import type { Theme } from './schema.ts';
import { themeToCssVars } from './tokens.ts';

/** Applies a theme to the document: CSS variables plus `data-theme` / `color-scheme`. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  for (const [name, value] of Object.entries(themeToCssVars(theme))) {
    root.style.setProperty(name, value);
  }
  root.dataset.theme = theme.id;
  root.dataset.appearance = theme.appearance;
  root.style.colorScheme = theme.appearance;
}
