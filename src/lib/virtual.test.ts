import { describe, expect, it } from 'vitest';

import { visibleRange } from './virtual.ts';

describe('visibleRange', () => {
  it('covers the viewport plus overscan', () => {
    // Rows 10–19 are visible; two extra on each side.
    expect(visibleRange(400, 400, 0, 1000, 40, 2)).toEqual({ start: 8, end: 22 });
  });

  it('accounts for content above the list', () => {
    // A 300 px header: the list is only partly visible at the top.
    expect(visibleRange(0, 400, 300, 100, 40, 0)).toEqual({ start: 0, end: 3 });
    // Scrolled past the header.
    expect(visibleRange(500, 400, 300, 100, 40, 0)).toEqual({ start: 5, end: 15 });
  });

  it('clamps to the list', () => {
    expect(visibleRange(0, 400, 0, 5, 40, 3)).toEqual({ start: 0, end: 5 });
    // Scrolled to the end.
    expect(visibleRange(1600, 400, 0, 50, 40, 3)).toEqual({ start: 37, end: 50 });
    // Scrolled past a list that just got shorter: nothing until the next scroll event.
    expect(visibleRange(10_000, 400, 0, 50, 40, 3)).toEqual({ start: 50, end: 50 });
    expect(visibleRange(0, 400, 0, 0, 40, 3)).toEqual({ start: 0, end: 0 });
  });

  it('renders nothing for a list below the viewport', () => {
    expect(visibleRange(0, 400, 900, 100, 40, 0)).toEqual({ start: 0, end: 0 });
  });
});
