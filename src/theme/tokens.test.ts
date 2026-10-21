import { describe, expect, it } from 'vitest';

import { EASINGS } from './schema.ts';
import { BUILT_IN_THEMES } from './themes.ts';
import { SPACE_STEPS, backdropColors, themeToCssVars } from './tokens.ts';

const aurora = BUILT_IN_THEMES.find((t) => t.id === 'aurora-night')!;

describe('themeToCssVars', () => {
  const vars = themeToCssVars(aurora);

  it('maps colors directly', () => {
    expect(vars['--vitra-color-background']).toBe('#05070D');
    expect(vars['--vitra-color-accent']).toBe('#3FA9F5');
  });

  it('adds units to lengths', () => {
    expect(vars['--vitra-glass-blur']).toBe('32px');
    expect(vars['--vitra-radius-lg']).toBe('22px');
    expect(vars['--vitra-font-size']).toBe('14px');
    expect(vars['--vitra-glass-shadow']).toBe('0 12px 48px #00000080');
  });

  it('derives the spacing scale from the unit', () => {
    for (const step of SPACE_STEPS) {
      expect(vars[`--vitra-space-${step}`]).toBe(`${step * 4}px`);
    }
  });

  it('resolves named easings and the backdrop cycle', () => {
    expect(vars['--vitra-motion-easing']).toBe(EASINGS.emphasized);
    expect(vars['--vitra-motion-duration']).toBe('260ms');
    expect(vars['--vitra-backdrop-duration']).toBe('69s'); // 48 s / 0.7
  });

  it('stops the backdrop at speed 0', () => {
    const still = { ...aurora, motion: { ...aurora.motion, backgroundSpeed: 0 } };
    expect(themeToCssVars(still)['--vitra-backdrop-duration']).toBe('0s');
  });

  it('exposes the aurora gradient as backdrop colors', () => {
    expect([1, 2, 3].map((i) => vars[`--vitra-backdrop-${i}`])).toEqual([
      '#2EE6C5',
      '#3FA9F5',
      '#7B5CFF',
    ]);
  });

  it('covers every theme with the same set of variables', () => {
    const keys = Object.keys(vars).sort();
    for (const theme of BUILT_IN_THEMES) {
      expect(Object.keys(themeToCssVars(theme)).sort()).toEqual(keys);
    }
  });
});

describe('backdropColors', () => {
  it('repeats short gradients and trims long ones', () => {
    expect(backdropColors(['#111', '#222'])).toEqual(['#111', '#222', '#111']);
    expect(backdropColors(['#1', '#2', '#3', '#4', '#5'])).toEqual(['#1', '#2', '#3']);
  });
});
