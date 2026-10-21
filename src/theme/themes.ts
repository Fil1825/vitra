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

/** Turns the theme setting into a concrete theme; unknown ids behave like `auto`. */
export function resolveTheme(themeId: string, systemPrefersDark: boolean): Theme {
  const theme = themeId === AUTO_THEME_ID ? undefined : byId.get(themeId);
  return theme ?? builtIn(systemPrefersDark ? 'dark' : 'light');
}
