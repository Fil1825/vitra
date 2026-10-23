import { describe, expect, it } from 'vitest';

import {
  COLUMN_WIDTH,
  DEFAULT_LAYOUT,
  MODULES,
  ZONES,
  isShown,
  layoutSchema,
  moveModule,
  normalizeLayout,
  setShown,
  setWeight,
  setWidth,
  shownIn,
  zoneOf,
} from './model.ts';

describe('normalizeLayout', () => {
  it('keeps a valid layout as it is', () => {
    expect(normalizeLayout(DEFAULT_LAYOUT)).toEqual(DEFAULT_LAYOUT);
    expect(layoutSchema.parse(DEFAULT_LAYOUT)).toEqual(DEFAULT_LAYOUT);
  });

  it('turns anything into the default layout', () => {
    for (const value of [undefined, null, 42, 'left', [], {}]) {
      expect(normalizeLayout(value)).toEqual(DEFAULT_LAYOUT);
    }
  });

  it('places every module exactly once', () => {
    const layout = normalizeLayout({
      zones: { left: ['sidebar', 'nope', 'sidebar'], right: ['lyrics', 'sidebar'] },
      hidden: [],
    });
    const placed = ZONES.flatMap((zone) => layout.zones[zone]);
    expect([...placed].sort()).toEqual([...MODULES].sort());
    expect(layout.zones.left).toEqual(['sidebar']);
    expect(layout.zones.right).toEqual(['lyrics', 'queue', 'artwork']);
  });

  it('sends a module home from a zone it does not fit, still shown', () => {
    const layout = normalizeLayout({
      zones: { left: ['controls', 'sidebar'], right: [], top: ['queue'], bottom: [] },
      hidden: ['lyrics', 'artwork', 'level'],
    });
    expect(layout.zones.left).toEqual(['sidebar']);
    expect(zoneOf(layout, 'controls')).toBe('bottom');
    expect(zoneOf(layout, 'queue')).toBe('right');
    expect(isShown(layout, 'queue')).toBe(true);
    expect(isShown(layout, 'controls')).toBe(true);
  });

  it('adds modules a layout never mentioned as Vitra has them by default', () => {
    const layout = normalizeLayout({ zones: { left: [], right: ['sidebar'] }, hidden: [] });
    expect(layout.zones.right[0]).toBe('sidebar');
    expect(isShown(layout, 'sidebar')).toBe(true);
    expect(isShown(layout, 'queue')).toBe(false);
    expect(isShown(layout, 'controls')).toBe(true);
  });

  it('keeps sizes within limits', () => {
    const layout = normalizeLayout({
      widths: { left: 10, right: 1e6 },
      weights: { queue: 99, lyrics: -1, artwork: 1, level: Number.NaN, nope: 2 },
    });
    expect(layout.widths).toEqual({ left: COLUMN_WIDTH.min, right: COLUMN_WIDTH.max });
    expect(layout.weights).toEqual({ queue: 4, lyrics: 0.25 });
  });
});

describe('editing a layout', () => {
  it('moves a module between zones and shows it', () => {
    const layout = moveModule(DEFAULT_LAYOUT, 'queue', 'left', 0);
    expect(layout.zones.left).toEqual(['queue', 'sidebar']);
    expect(layout.zones.right).toEqual(['lyrics', 'artwork']);
    expect(isShown(layout, 'queue')).toBe(true);
  });

  it('reorders within a zone', () => {
    const layout = moveModule(DEFAULT_LAYOUT, 'artwork', 'right', 0);
    expect(layout.zones.right).toEqual(['artwork', 'queue', 'lyrics']);
  });

  it('refuses a zone the module does not fit', () => {
    expect(moveModule(DEFAULT_LAYOUT, 'controls', 'left', 0)).toBe(DEFAULT_LAYOUT);
    expect(moveModule(DEFAULT_LAYOUT, 'sidebar', 'top', 0)).toBe(DEFAULT_LAYOUT);
    expect(zoneOf(moveModule(DEFAULT_LAYOUT, 'level', 'top', 5), 'level')).toBe('top');
  });

  it('shows and hides modules where they are', () => {
    const shown = setShown(DEFAULT_LAYOUT, 'lyrics', true);
    expect(shownIn(shown, 'right')).toEqual(['lyrics']);
    expect(setShown(shown, 'lyrics', false)).toEqual(DEFAULT_LAYOUT);
  });

  it('keeps widths and weights within limits, and drops weight 1', () => {
    expect(setWidth(DEFAULT_LAYOUT, 'right', 9000).widths.right).toBe(COLUMN_WIDTH.max);
    expect(setWeight(DEFAULT_LAYOUT, 'queue', 2).weights).toEqual({ queue: 2 });
    expect(setWeight(setWeight(DEFAULT_LAYOUT, 'queue', 2), 'queue', 1).weights).toEqual({});
  });
});
