import { describe, expect, it } from 'vitest';

import { gapAt, moveTarget } from './reorder.ts';

describe('gapAt', () => {
  it('rounds to the nearest gap between rows', () => {
    expect(gapAt(0, 40, 5)).toBe(0);
    expect(gapAt(19, 40, 5)).toBe(0);
    expect(gapAt(21, 40, 5)).toBe(1);
    expect(gapAt(130, 40, 5)).toBe(3);
  });

  it('stays within the list', () => {
    expect(gapAt(-100, 40, 5)).toBe(0);
    expect(gapAt(10_000, 40, 5)).toBe(5);
  });
});

describe('moveTarget', () => {
  it('moves up into the gap', () => {
    expect(moveTarget(4, 1)).toBe(1);
  });

  it('moves down, counting the removed row', () => {
    // Gap 4 sits between rows 3 and 4; once row 1 is removed that is index 3.
    expect(moveTarget(1, 4)).toBe(3);
    expect(moveTarget(1, 5)).toBe(4);
  });

  it('ignores drops right around the row itself', () => {
    expect(moveTarget(2, 2)).toBeNull();
    expect(moveTarget(2, 3)).toBeNull();
  });
});
