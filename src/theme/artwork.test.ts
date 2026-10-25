import { describe, expect, it } from 'vitest';

import { vinylOn } from './artwork.ts';
import { BUILT_IN_THEMES } from './themes.ts';

const dark = BUILT_IN_THEMES.find((t) => t.id === 'dark')!;
const vinylTheme = { ...dark, artwork: { shape: 'circle' as const, vinyl: true } };

describe('vinyl', () => {
  it('follows the theme unless the user chose', () => {
    expect(vinylOn(dark, 'auto')).toBe(false);
    expect(vinylOn(vinylTheme, 'auto')).toBe(true);
    expect(vinylOn(dark, 'on')).toBe(true);
    expect(vinylOn(vinylTheme, 'off')).toBe(false);
  });
});
