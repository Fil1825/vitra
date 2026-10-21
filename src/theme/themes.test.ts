import { describe, expect, it } from 'vitest';

import { themeSchema } from './schema.ts';
import { AUTO_THEME_ID, BUILT_IN_THEMES, resolveTheme } from './themes.ts';
import dark from './builtin/dark.json';

describe('built-in themes', () => {
  it('include Light, Dark and Aurora Night', () => {
    expect(BUILT_IN_THEMES.map((t) => t.id)).toEqual(['light', 'dark', 'aurora-night']);
  });

  it.each(BUILT_IN_THEMES.map((t) => [t.id, t] as const))('%s is a valid theme', (_, theme) => {
    expect(themeSchema.safeParse(theme).success).toBe(true);
  });

  it('Aurora Night uses the brand palette', () => {
    const aurora = BUILT_IN_THEMES.find((t) => t.id === 'aurora-night');
    expect(aurora?.colors.accent).toBe('#3FA9F5');
    expect(aurora?.background.gradient).toEqual(['#2EE6C5', '#3FA9F5', '#7B5CFF']);
    expect(aurora?.accentFromArtwork).toBe(false);
  });
});

describe('resolveTheme', () => {
  it('follows the system for auto', () => {
    expect(resolveTheme(AUTO_THEME_ID, true).id).toBe('dark');
    expect(resolveTheme(AUTO_THEME_ID, false).id).toBe('light');
  });

  it('returns the chosen theme regardless of the system', () => {
    expect(resolveTheme('aurora-night', false).id).toBe('aurora-night');
  });

  it('treats unknown ids like auto', () => {
    expect(resolveTheme('deleted-theme', true).id).toBe('dark');
  });
});

describe('themeSchema', () => {
  const withChange = (change: (theme: Record<string, unknown>) => void) => {
    const theme = structuredClone(dark) as unknown as Record<string, unknown>;
    change(theme);
    return themeSchema.safeParse(theme);
  };

  it('rejects colors that are not hex', () => {
    const result = withChange((t) => {
      (t.colors as Record<string, unknown>).accent = 'red; background: url(x)';
    });
    expect(result.success).toBe(false);
  });

  it('rejects font stacks that could escape the CSS declaration', () => {
    for (const fontFamily of ['Inter; color: red', 'x} body {', 'url(https://evil)']) {
      const result = withChange((t) => {
        (t.typography as Record<string, unknown>).fontFamily = fontFamily;
      });
      expect(result.success, fontFamily).toBe(false);
    }
  });

  it('accepts quoted font names', () => {
    const result = withChange((t) => {
      (t.typography as Record<string, unknown>).fontFamily =
        `"Inter Display", 'Segoe UI', sans-serif`;
    });
    expect(result.success).toBe(true);
  });

  it('rejects out-of-range glass values and unknown easings', () => {
    expect(withChange((t) => ((t.glass as Record<string, unknown>).opacity = 1.5)).success).toBe(
      false,
    );
    expect(
      withChange((t) => ((t.motion as Record<string, unknown>).easing = 'steps(2)')).success,
    ).toBe(false);
  });

  it('rejects ids that are not slugs', () => {
    expect(withChange((t) => (t.id = '../themes/x')).success).toBe(false);
  });
});
