import { themeSchema, type Theme } from './schema.ts';
import auroraNight from './builtin/aurora-night.json';
import dark from './builtin/dark.json';
import light from './builtin/light.json';

/** Follows the system light/dark setting. */
export const AUTO_THEME_ID = 'auto';

/** Built-in themes, validated like any imported theme so they double as schema examples. */
export const BUILT_IN_THEMES: readonly Theme[] = [light, dark, auroraNight].map((t) =>
  themeSchema.parse(t),
);

const byId = new Map(BUILT_IN_THEMES.map((t) => [t.id, t]));

function builtIn(id: string): Theme {
  const theme = byId.get(id);
  if (!theme) {
    throw new Error(`Built-in theme "${id}" is missing`);
  }
  return theme;
}

/** Whether `id` is a built-in theme (or `auto`), which user themes can't replace. */
export function isBuiltIn(id: string): boolean {
  return id === AUTO_THEME_ID || byId.has(id);
}

/**
 * Turns the theme setting into a concrete theme, looking at the user's themes after the
 * built-in ones; unknown ids (a removed theme) behave like `auto`.
 */
export function resolveTheme(
  themeId: string,
  systemPrefersDark: boolean,
  userThemes: readonly Theme[] = [],
): Theme {
  const theme =
    themeId === AUTO_THEME_ID
      ? undefined
      : (byId.get(themeId) ?? userThemes.find((t) => t.id === themeId));
  return theme ?? builtIn(systemPrefersDark ? 'dark' : 'light');
}
