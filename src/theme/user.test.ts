import { describe, expect, it } from 'vitest';

import { BUILT_IN_THEMES } from './themes.ts';
import {
  checkThemeJson,
  readInstalled,
  serializeTheme,
  themeStyleSheet,
  uniqueThemeId,
  withAssets,
} from './user.ts';

const dark = BUILT_IN_THEMES.find((theme) => theme.id === 'dark');
if (!dark) throw new Error('no dark theme');

const sunset = {
  ...dark,
  id: 'sunset',
  name: 'Sunset',
  typography: {
    ...dark.typography,
    fontFamily: "'Inter Display', system-ui",
    fonts: [{ family: 'Inter Display', src: 'assets/inter.woff2', weight: '100 900' }],
  },
  background: {
    ...dark.background,
    image: { src: 'assets/dunes.jpg', opacity: 0.6, fit: 'cover' as const },
  },
};

describe('user themes', () => {
  it('checks theme.json and says what is wrong', () => {
    expect(checkThemeJson(serializeTheme(sunset))).toEqual({ ok: true, theme: sunset });
    const broken = checkThemeJson(
      JSON.stringify({ ...sunset, colors: { ...sunset.colors, accent: 'red' }, glass: 1 }),
    );
    expect(broken.ok).toBe(false);
    expect(!broken.ok && broken.error).toMatch(/colors\.accent: Expected a hex color/);
    expect(!broken.ok && broken.error).toMatch(/glass:/);
    expect(checkThemeJson('{ nope').ok).toBe(false);
  });

  it("takes the folder's id and drops assets the theme doesn't have", () => {
    const read = readInstalled({
      id: 'sunset-2',
      json: serializeTheme(sunset),
      css: null,
      assets: ['dunes.jpg'],
    });
    if ('error' in read) throw new Error(read.error);
    expect(read.theme.id).toBe('sunset-2');
    expect(read.theme.background.image?.src).toBe('assets/dunes.jpg');
    expect(read.theme.typography.fonts).toBeUndefined();
    expect(withAssets(sunset, []).background.image).toBeUndefined();
  });

  it('turns fonts and custom.css into a style sheet with theme addresses', () => {
    const { css, removed } = themeStyleSheet(
      {
        theme: sunset,
        css: 'body { background: url(assets/dunes.jpg) } a { background: url(https://x.example/p.png) }',
        assets: ['inter.woff2', 'dunes.jpg'],
      },
      (name) => `theme://localhost/sunset/${name}`,
    );
    expect(css).toContain(
      '@font-face { font-family: "Inter Display"; src: url("theme://localhost/sunset/inter.woff2"); font-weight: 100 900;',
    );
    expect(css).toContain('url("theme://localhost/sunset/dunes.jpg")');
    expect(css).not.toContain('x.example');
    expect(removed).toHaveLength(1);
  });

  it('finds a free id that is never a built-in one', () => {
    const taken = new Set(['sunset', 'sunset-2']);
    expect(uniqueThemeId('sunset', taken)).toBe('sunset-3');
    expect(uniqueThemeId('My Theme!', taken)).toBe('my-theme');
    expect(uniqueThemeId('dark', taken)).toBe('dark-2');
    expect(uniqueThemeId('---', taken)).toBe('theme');
  });
});
