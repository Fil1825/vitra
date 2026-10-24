import { describe, expect, it } from 'vitest';

import { DEFAULT_SETTINGS, EQ_BANDS, EQ_GAIN_MAX } from '../../lib/settings.ts';
import {
  BUILTIN_IDS,
  BUILTIN_PRESETS,
  bandLabel,
  clampGain,
  gainLabel,
  headroom,
  presetCurve,
  userPresetId,
  withGain,
} from './presets.ts';

describe('equalizer presets', () => {
  it('give every band a gain in range', () => {
    for (const id of BUILTIN_IDS) {
      const gains = BUILTIN_PRESETS[id];
      expect(gains, id).toHaveLength(EQ_BANDS.length);
      for (const gain of gains) expect(Math.abs(gain), id).toBeLessThanOrEqual(EQ_GAIN_MAX);
    }
  });

  it('leave room for their boosts', () => {
    expect(headroom([0, -3])).toBe(0);
    expect(headroom([2, 5.5, 1])).toBe(-5.5);
    expect(headroom([4.2])).toBe(-4.5);
    expect(presetCurve('rock', [])).toEqual({ gains: [...BUILTIN_PRESETS.rock], preamp: -5 });
  });

  it('find the curves the user saved', () => {
    const saved = [{ name: 'Car', gains: EQ_BANDS.map(() => 2), preamp: -2 }];
    expect(presetCurve(userPresetId('Car'), saved)).toEqual({
      gains: EQ_BANDS.map(() => 2),
      preamp: -2,
    });
    expect(presetCurve(userPresetId('Gone'), saved)).toBeNull();
    expect(presetCurve('nonsense', saved)).toBeNull();
  });

  it('forget the preset once a band moves', () => {
    const eq = { ...DEFAULT_SETTINGS.sound.equalizer, preset: 'rock' };
    const moved = withGain(eq, 3, 4.26);
    expect(moved.gains[3]).toBe(4.3);
    expect(moved.preset).toBe('');
    expect(withGain(eq, -1, -20).preamp).toBe(-EQ_GAIN_MAX);
    expect(clampGain(Number.NaN)).toBe(0);
  });

  it('label bands and gains', () => {
    expect(EQ_BANDS.map(bandLabel)).toEqual([
      '32',
      '64',
      '125',
      '250',
      '500',
      '1k',
      '2k',
      '4k',
      '8k',
      '16k',
    ]);
    expect([0, 3, 3.5, -2].map(gainLabel)).toEqual(['0', '+3', '+3.5', '−2']);
  });
});
