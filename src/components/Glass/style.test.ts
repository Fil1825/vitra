import { describe, expect, it } from 'vitest';

import { glassStyle } from './style.ts';

describe('glassStyle', () => {
  it('leaves theme defaults alone when no option is given', () => {
    expect(glassStyle({})).toEqual({});
  });

  it('turns options into --glass-* custom properties', () => {
    expect(
      glassStyle({
        blur: 12,
        saturation: 1.4,
        tint: '#0B1426',
        opacity: 0.3,
        highlight: 0.8,
        radius: 'md',
      }),
    ).toEqual({
      '--glass-blur': '12px',
      '--glass-saturation': '1.4',
      '--glass-tint': '#0B1426',
      '--glass-opacity': '0.3',
      '--glass-highlight': '0.8',
      '--glass-radius': 'var(--vitra-radius-md)',
    });
  });

  it('handles numeric radius and border variants', () => {
    expect(glassStyle({ radius: 6, border: false })).toEqual({
      '--glass-radius': '6px',
      '--glass-border': 'transparent',
    });
    expect(glassStyle({ border: '#CFEFFF40' })).toEqual({ '--glass-border': '#CFEFFF40' });
    expect(glassStyle({ border: true })).toEqual({});
  });
});
