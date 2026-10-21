import { describe, expect, it } from 'vitest';

import { allocateSlots, type GlassSlot } from './budget.ts';

const slots = (entries: [string, number, number][]) =>
  new Map<string, GlassSlot>(entries.map(([id, priority, order]) => [id, { priority, order }]));

describe('allocateSlots', () => {
  it('gives every layer a slot while under budget', () => {
    expect(
      allocateSlots(
        slots([
          ['a', 0, 0],
          ['b', 0, 1],
        ]),
        8,
      ),
    ).toEqual(new Set(['a', 'b']));
  });

  it('keeps the highest priorities when over budget', () => {
    const allowed = allocateSlots(
      slots([
        ['card', 1, 0],
        ['sidebar', 2, 1],
        ['controls', 3, 2],
        ['tooltip', 0, 3],
      ]),
      2,
    );
    expect(allowed).toEqual(new Set(['controls', 'sidebar']));
  });

  it('breaks priority ties by registration order', () => {
    expect(
      allocateSlots(
        slots([
          ['late', 1, 5],
          ['early', 1, 2],
        ]),
        1,
      ),
    ).toEqual(new Set(['early']));
  });

  it('allows nothing with a zero budget', () => {
    expect(allocateSlots(slots([['a', 9, 0]]), 0).size).toBe(0);
  });
});
