import { describe, expect, it } from 'vitest';

import { Dynamics, barCount, idleLevels, resample } from './spectrum.ts';

describe('the level meter', () => {
  it('shows as many bars as its width has room for', () => {
    expect(barCount(40)).toBe(8);
    expect(barCount(240)).toBe(20);
    expect(barCount(2000)).toBe(32);
  });

  it('spreads the bands over its bars', () => {
    const bands = [0, 0.5, 1];
    expect(resample(bands, 3)).toEqual([0, 0.5, 1]);
    expect(resample(bands, 5)).toEqual([0, 0.25, 0.5, 0.75, 1]);
    expect(resample([], 4)).toEqual([0, 0, 0, 0]);
    const many = resample(
      Array.from({ length: 20 }, (_, i) => i / 19),
      32,
    );
    expect(many).toHaveLength(32);
    expect(many[0]).toBe(0);
    expect(many[31]).toBeCloseTo(1);
  });

  it('rises fast and falls slowly', () => {
    const dynamics = new Dynamics();
    dynamics.step([1], 1 / 60);
    // A sixtieth of a second takes it a good part of the way up.
    expect(dynamics.values[0]).toBeGreaterThan(0.25);
    for (let i = 0; i < 30; i++) dynamics.step([1], 1 / 60);
    expect(dynamics.values[0]).toBeGreaterThan(0.99);
    dynamics.step([0], 1 / 60);
    expect(dynamics.values[0]).toBeGreaterThan(0.9);
  });

  it('holds a peak a moment, then lets it fall', () => {
    const dynamics = new Dynamics();
    for (let i = 0; i < 30; i++) dynamics.step([0.8], 1 / 60);
    for (let i = 0; i < 12; i++) dynamics.step([0], 1 / 60);
    // Still held after a fifth of a second, while the bar is already lower.
    expect(dynamics.peaks[0]).toBeCloseTo(0.8, 2);
    expect(dynamics.values[0]).toBeLessThan(0.6);
    for (let i = 0; i < 120; i++) dynamics.step([0], 1 / 60);
    expect(dynamics.peaks[0]).toBeLessThan(0.1);
    expect(dynamics.peaks[0]).toBeGreaterThanOrEqual(dynamics.values[0] ?? 0);
  });

  it('settles, so drawing can stop', () => {
    const dynamics = new Dynamics();
    expect(dynamics.settled([0.2])).toBe(false);
    for (let i = 0; i < 600; i++) dynamics.step([0.2], 1 / 60);
    expect(dynamics.settled([0.2])).toBe(true);
  });

  it('breathes quietly when nothing plays', () => {
    for (const seconds of [0, 1.7, 9.3]) {
      for (const level of idleLevels(16, seconds)) {
        expect(level).toBeGreaterThan(0.05);
        expect(level).toBeLessThan(0.3);
      }
    }
    expect(idleLevels(16, 0)).not.toEqual(idleLevels(16, 1));
  });
});
