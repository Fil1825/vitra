import type { z } from 'zod';

import type { InstalledTheme } from '../lib/themes.ts';
import { sanitizeCss } from './css.ts';
import { themeSchema, type Theme } from './schema.ts';
import { isBuiltIn } from './themes.ts';

/** An installed theme that passed the schema. */
export interface UserTheme {
  theme: Theme;
  /** `custom.css` as stored; cleaned again whenever it is used. */
  css: string | null;
  /** File names in the theme's `assets/` folder. */
  assets: readonly string[];
}

/** Turns an asset file name into the address the page loads it from. */
export type AssetUrl = (name: string) => string;

/** A readable list of what is wrong with a theme, one problem per line. */
export function describeIssues(error: z.ZodError, limit = 6): string {
  const lines = error.issues.slice(0, limit).map((issue) => {
    const path = issue.path.join('.');
    return path ? `${path}: ${issue.message}` : issue.message;
  });
  if (error.issues.length > limit) {
    lines.push(`… ${error.issues.length - limit} more`);
  }
  return lines.join('\n');
}

export type Checked = { ok: true; theme: Theme } | { ok: false; error: string };

/** Parses and validates `theme.json`. */
export function checkThemeJson(json: string): Checked {
  let value: unknown;
  try {
    value = JSON.parse(json);
  } catch (err) {
    return { ok: false, error: `theme.json is not valid JSON: ${String(err)}` };
  }
  const result = themeSchema.safeParse(value);
  return result.success
    ? { ok: true, theme: result.data }
    : { ok: false, error: describeIssues(result.error) };
}

/**
 * An installed theme ready to use, or why it can't be. Its id is its folder's (a renamed
 * folder still works), and references to assets it doesn't have are dropped.
 */
export function readInstalled(installed: InstalledTheme): UserTheme | { error: string } {
  const checked = checkThemeJson(installed.json);
  if (!checked.ok) {
    return { error: checked.error };
  }
  return {
    theme: withAssets({ ...checked.theme, id: installed.id }, installed.assets),
    css: installed.css,
    assets: installed.assets,
  };
}

const assetName = (path: string) => path.slice('assets/'.length);

/** The theme without references to assets it doesn't have. */
export function withAssets(theme: Theme, assets: readonly string[]): Theme {
  const has = (path: string) => assets.includes(assetName(path));
  const fonts = theme.typography.fonts?.filter((font) => has(font.src));
  const image =
    theme.background.image && has(theme.background.image.src) ? theme.background.image : undefined;
  return {
    ...theme,
    typography: { ...theme.typography, fonts: fonts?.length ? fonts : undefined },
    background: { ...theme.background, image },
  };
}

/**
 * The style sheet a user theme adds to the page: its fonts, then its cleaned `custom.css`.
 * Returns what the cleaning took out too.
 */
export function themeStyleSheet(
  user: UserTheme,
  assetUrl: AssetUrl,
): { css: string; removed: string[] } {
  const fonts = (user.theme.typography.fonts ?? []).map(
    (font) =>
      `@font-face { font-family: ${JSON.stringify(font.family)}; ` +
      `src: url(${JSON.stringify(assetUrl(assetName(font.src)))}); ` +
      `font-weight: ${font.weight ?? 'normal'}; font-style: ${font.style ?? 'normal'}; ` +
      'font-display: swap; }',
  );
  const custom = user.css
    ? sanitizeCss(user.css, (path) =>
        user.assets.includes(assetName(path)) ? assetUrl(assetName(path)) : null,
      )
    : { css: '', removed: [] };
  return { css: [...fonts, custom.css].filter(Boolean).join('\n'), removed: custom.removed };
}

/** `theme.json` as Vitra writes it. */
export function serializeTheme(theme: Theme): string {
  return `${JSON.stringify(theme, null, 2)}\n`;
}

/** `base` made into an id, or `base-2`, `base-3`… whichever is free (built-in ids never are). */
export function uniqueThemeId(base: string, taken: ReadonlySet<string>): string {
  const stem =
    base
      .toLowerCase()
      .replace(/[^a-z0-9-]+/g, '-')
      .replace(/^-+|-+$/g, '')
      .slice(0, 58) || 'theme';
  const free = (id: string) => !taken.has(id) && !isBuiltIn(id);
  if (free(stem)) {
    return stem;
  }
  for (let n = 2; ; n += 1) {
    if (free(`${stem}-${n}`)) return `${stem}-${n}`;
  }
}
