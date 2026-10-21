import { describe, expect, it } from 'vitest';

import { EMPTY_SELECTION, select, selectForMenu, selectedRows } from './selection.ts';

const plain = { toggle: false, extend: false };
const toggle = { toggle: true, extend: false };
const extend = { toggle: false, extend: true };

describe('select', () => {
  it('a plain click selects one row', () => {
    const one = select(select(EMPTY_SELECTION, 3, plain), 5, plain);
    expect(selectedRows(one)).toEqual([5]);
    expect(one.anchor).toBe(5);
  });

  it('toggles rows in and out', () => {
    let selection = select(EMPTY_SELECTION, 1, plain);
    selection = select(selection, 4, toggle);
    expect(selectedRows(selection)).toEqual([1, 4]);
    selection = select(selection, 1, toggle);
    expect(selectedRows(selection)).toEqual([4]);
  });

  it('extends a range from the anchor in either direction', () => {
    const anchored = select(EMPTY_SELECTION, 5, plain);
    expect(selectedRows(select(anchored, 8, extend))).toEqual([5, 6, 7, 8]);
    expect(selectedRows(select(anchored, 3, extend))).toEqual([3, 4, 5]);
  });

  it('a range without an anchor is a plain click', () => {
    expect(selectedRows(select(EMPTY_SELECTION, 2, extend))).toEqual([2]);
  });
});

describe('selectForMenu', () => {
  it('keeps a selection that contains the row', () => {
    const selection = select(select(EMPTY_SELECTION, 1, plain), 3, extend);
    expect(selectForMenu(selection, 2)).toBe(selection);
    expect(selectedRows(selectForMenu(selection, 7))).toEqual([7]);
  });
});
