import { describe, expect, it } from 'vitest';

import { LP_SPEED } from '../../lib/settings.ts';
import { degreesPerSecond, nextVelocity, SPIN_DOWN_S, SPIN_UP_S } from './platter.ts';
import { DETENTS, DIAL_MAX, DIAL_MIN, formatSpeed, positionOf, speedAt } from './speedDial.ts';

describe('record speed slider', () => {
  it('covers 5 to 78 turns a minute', () => {
    expect(speedAt(DIAL_MIN)).toBe(5);
    expect(speedAt(DIAL_MAX)).toBe(78);
  });

  it('holds the classic speeds for a stretch around them', () => {
    for (const detent of DETENTS) {
      const at = positionOf(detent);
      expect(speedAt(at)).toBe(detent);
      expect(speedAt(at - 1)).toBe(detent);
      expect(speedAt(at + 1)).toBe(detent);
    }
    // Just past a detent's stretch the speed carries on from it.
    expect(speedAt(positionOf(LP_SPEED) + 2)).toBeCloseTo(LP_SPEED + 0.5, 1);
    expect(speedAt(positionOf(45) - 2)).toBeCloseTo(44.5, 1);
  });

  it('puts every speed back where it was', () => {
    for (const speed of [5, 20, 33, 33.5, 40, 44.5, 45, 46, 60, 77.5, 78]) {
      expect(speedAt(positionOf(speed))).toBeCloseTo(speed, 5);
    }
    expect(speedAt(positionOf(LP_SPEED))).toBe(LP_SPEED);
  });

  it('never gets stuck on a detent with the arrow keys', () => {
    const speeds = new Set<number>();
    for (let position = DIAL_MIN; position <= DIAL_MAX; position += 0.5) {
      speeds.add(speedAt(position));
    }
    expect(speeds.has(33)).toBe(true);
    expect(speeds.has(33.5)).toBe(true);
    expect(speeds.has(46)).toBe(true);
  });

  it('writes speeds as people do', () => {
    expect(formatSpeed(LP_SPEED, 'en')).toBe('33⅓');
    expect(formatSpeed(45, 'ru')).toBe('45');
    expect(formatSpeed(52.5, 'en')).toBe('52.5');
    expect(formatSpeed(52.5, 'ru')).toBe('52,5');
  });
});

describe('platter', () => {
  it('turns 33⅓ times a minute', () => {
    expect(degreesPerSecond(LP_SPEED)).toBeCloseTo(200, 5);
  });

  /** Seconds the platter takes from `from` to `to` (degrees a second) at a record speed of `full`. */
  const timeTo = (from: number, to: number, full: number) => {
    let velocity = from;
    let t = 0;
    while (velocity !== to && t < 10) {
      velocity = nextVelocity(velocity, to, full, 1 / 60);
      t += 1 / 60;
    }
    return t;
  };

  it('gets going and stops in the same time at any speed', () => {
    for (const rpm of [5, LP_SPEED, 45, 78]) {
      const full = degreesPerSecond(rpm);
      expect(timeTo(0, full, full)).toBeCloseTo(SPIN_UP_S, 1);
      expect(timeTo(full, 0, full)).toBeCloseTo(SPIN_DOWN_S, 1);
    }
  });

  it('eases to a new speed instead of jumping', () => {
    const slow = degreesPerSecond(LP_SPEED);
    const fast = degreesPerSecond(78);
    const sped = nextVelocity(slow, fast, fast, 1 / 60);
    expect(sped).toBeGreaterThan(slow);
    expect(sped).toBeLessThan(fast);
    const slowed = nextVelocity(fast, slow, slow, 1 / 60);
    expect(slowed).toBeLessThan(fast);
    expect(slowed).toBeGreaterThan(slow);
    expect(timeTo(fast, slow, slow)).toBeLessThan(SPIN_DOWN_S + 0.1);
  });
});
