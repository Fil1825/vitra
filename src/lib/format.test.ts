import { describe, expect, it } from 'vitest';

import { formatSpeed, formatTime, formatTotal } from './format.ts';

describe('formatTime', () => {
  it('formats minutes and seconds', () => {
    expect(formatTime(0)).toBe('0:00');
    expect(formatTime(7.9)).toBe('0:07');
    expect(formatTime(65)).toBe('1:05');
    expect(formatTime(3599)).toBe('59:59');
  });

  it('adds hours for long tracks', () => {
    expect(formatTime(3600)).toBe('1:00:00');
    expect(formatTime(3725)).toBe('1:02:05');
  });

  it('treats invalid values as zero', () => {
    expect(formatTime(-3)).toBe('0:00');
    expect(formatTime(Number.NaN)).toBe('0:00');
  });
});

describe('formatSpeed', () => {
  it('drops trailing zeros', () => {
    expect(formatSpeed(1)).toBe('1×');
    expect(formatSpeed(1.25)).toBe('1.25×');
    expect(formatSpeed(0.5)).toBe('0.5×');
  });
});

describe('formatTotal', () => {
  const units = { hours: 'h', minutes: 'min' };

  it('shows minutes, then hours and minutes', () => {
    expect(formatTotal(0, units)).toBe('0 min');
    expect(formatTotal(12, units)).toBe('1 min');
    expect(formatTotal(42 * 60 + 20, units)).toBe('42 min');
    expect(formatTotal(3600 + 5 * 60, units)).toBe('1 h 05 min');
  });
});
