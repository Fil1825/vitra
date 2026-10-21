import { describe, expect, it } from 'vitest';

import { accentFor } from './accent.ts';

const lightnessOf = (hex: string) => {
  const n = Number.parseInt(hex.slice(1), 16);
  const c = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map((v) => v / 255);
  return (Math.max(...c) + Math.min(...c)) / 2;
};

describe('accentFor', () => {
  it('lightens dark artwork colors on dark themes', () => {
    const { accent } = accentFor('#3A0B10', 'dark');
    expect(lightnessOf(accent)).toBeGreaterThanOrEqual(0.57);
  });

  it('darkens pale artwork colors on light themes', () => {
    const { accent } = accentFor('#FFF2B0', 'light');
    expect(lightnessOf(accent)).toBeLessThanOrEqual(0.49);
  });

  it('keeps the hue', () => {
    const { accent } = accentFor('#1F86E0', 'dark');
    const n = Number.parseInt(accent.slice(1), 16);
    const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    expect(b).toBeGreaterThan(g);
    expect(g).toBeGreaterThan(r);
  });

  it('leaves greys grey', () => {
    expect(accentFor('#808080', 'dark').accent).toMatch(/^#([0-9A-F]{2})\1\1$/);
  });

  it('picks readable text for the accent', () => {
    expect(accentFor('#F5D020', 'dark').contrast).toBe('#0B0D12');
    expect(accentFor('#1F3FA0', 'light').contrast).toBe('#FFFFFF');
  });
});
