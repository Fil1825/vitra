import type { Theme } from './schema.ts';
import { themeToCssVars } from './tokens.ts';

const STYLE_ID = 'vitra-theme-css';

/**
 * Puts a user theme's fonts and cleaned `custom.css` into the page, after every other style
 * sheet so it wins ties; an empty sheet removes it.
 */
export function applyThemeStyleSheet(css: string, doc: Document = document): void {
  let element = doc.getElementById(STYLE_ID);
  if (!css) {
    element?.remove();
    return;
  }
  if (!element) {
    element = doc.createElement('style');
    element.id = STYLE_ID;
  }
  // Appended again each time: style sheets loaded later (Vite's, in development) mustn't win.
  doc.head.append(element);
  // textContent, never innerHTML: the text stays CSS whatever it contains.
  element.textContent = css;
}

/** Applies a theme to the document: CSS variables plus `data-theme` / `color-scheme`. */
export function applyTheme(theme: Theme, root: HTMLElement = document.documentElement): void {
  for (const [name, value] of Object.entries(themeToCssVars(theme))) {
    root.style.setProperty(name, value);
  }
  root.dataset.theme = theme.id;
  root.dataset.appearance = theme.appearance;
  root.style.colorScheme = theme.appearance;
}
