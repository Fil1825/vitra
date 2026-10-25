import { describe, expect, it } from 'vitest';

import type { ModuleId, Zone } from './model.ts';
import {
  isMoving,
  nextLayoutPresence,
  nextPresence,
  settle,
  settledLayout,
  settledPresence,
  settleLayout,
  splitPhase,
  zoneMotion,
  type Presence,
} from './presence.ts';

const shown = (zones: Partial<Record<Zone, ModuleId[]>>): Record<Zone, ModuleId[]> => ({
  top: [],
  left: [],
  right: [],
  bottom: [],
  ...zones,
});

const phases = (entries: Presence[]) => entries.map((e) => `${e.module}:${e.phase}`);

describe('module presence', () => {
  it('lets a hidden module leave from where it was', () => {
    const before = settledPresence(['sidebar', 'queue', 'artwork']);
    expect(phases(nextPresence(before, ['sidebar', 'artwork']))).toEqual([
      'sidebar:present',
      'queue:leaving',
      'artwork:present',
    ]);
    expect(phases(nextPresence(before, ['queue', 'artwork']))).toEqual([
      'sidebar:leaving',
      'queue:present',
      'artwork:present',
    ]);
  });

  it('brings a shown module in, and one coming back while it leaves', () => {
    const before = settledPresence(['sidebar']);
    const added = nextPresence(before, ['sidebar', 'queue']);
    expect(phases(added)).toEqual(['sidebar:present', 'queue:entering']);
    const hidden = nextPresence(settle(added, 'queue'), ['sidebar']);
    expect(phases(nextPresence(hidden, ['sidebar', 'queue']))).toEqual([
      'sidebar:present',
      'queue:entering',
    ]);
  });

  it('settles one module at a time', () => {
    const moving = nextPresence(settledPresence(['sidebar', 'queue']), ['lyrics', 'queue']);
    expect(phases(moving)).toEqual(['sidebar:leaving', 'lyrics:entering', 'queue:present']);
    expect(phases(settle(moving, 'sidebar'))).toEqual(['lyrics:entering', 'queue:present']);
    expect(phases(settle(settle(moving, 'sidebar'), 'lyrics'))).toEqual([
      'lyrics:present',
      'queue:present',
    ]);
  });

  it('opens and closes a column that was or becomes empty', () => {
    expect(zoneMotion(nextPresence([], ['sidebar']))).toBe('opening');
    expect(zoneMotion(nextPresence(settledPresence(['sidebar']), []))).toBe('closing');
    expect(zoneMotion(nextPresence(settledPresence(['sidebar']), ['sidebar', 'queue']))).toBe(
      undefined,
    );
    expect(zoneMotion([])).toBe(undefined);
  });

  it('moves the line next to a module coming or going', () => {
    const middle = nextPresence(settledPresence(['sidebar', 'queue', 'artwork']), [
      'sidebar',
      'artwork',
    ]);
    expect([1, 2].map((i) => splitPhase(middle, i))).toEqual(['leaving', 'present']);
    const first = nextPresence(settledPresence(['sidebar', 'queue']), ['queue']);
    expect(splitPhase(first, 1)).toBe('leaving');
  });

  it('never shows a moved module twice', () => {
    const before = settledLayout(shown({ left: ['sidebar', 'queue'], right: ['lyrics'] }));
    const after = nextLayoutPresence(
      before,
      shown({ left: ['sidebar'], right: ['queue', 'lyrics'] }),
    );
    expect(phases(after.left)).toEqual(['sidebar:present']);
    expect(phases(after.right)).toEqual(['queue:entering', 'lyrics:present']);
  });

  it('changes the bars at once', () => {
    const before = settledLayout(shown({ bottom: ['controls', 'level'] }));
    const after = nextLayoutPresence(before, shown({ bottom: ['controls'] }));
    expect(phases(after.bottom)).toEqual(['controls:present']);
    expect(isMoving(after)).toBe(false);
  });

  it('can settle everything at once', () => {
    const before = settledLayout(shown({ left: ['sidebar'], right: ['queue'] }));
    const after = nextLayoutPresence(before, shown({ left: [], right: ['queue', 'lyrics'] }));
    expect(isMoving(after)).toBe(true);
    const done = settleLayout(after);
    expect(isMoving(done)).toBe(false);
    expect(phases(done.left)).toEqual([]);
    expect(phases(done.right)).toEqual(['queue:present', 'lyrics:present']);
  });
});
