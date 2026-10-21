/** Row selection in a list: plain click, Cmd/Ctrl-click toggles, Shift-click extends. */
export interface Selection {
  /** Selected row indices. */
  rows: ReadonlySet<number>;
  /** Where a Shift-click range starts. */
  anchor: number | null;
}

export const EMPTY_SELECTION: Selection = { rows: new Set(), anchor: null };

export interface Modifiers {
  toggle: boolean;
  extend: boolean;
}

export function select(selection: Selection, index: number, modifiers: Modifiers): Selection {
  if (modifiers.extend && selection.anchor !== null) {
    const [from, to] =
      selection.anchor <= index ? [selection.anchor, index] : [index, selection.anchor];
    const rows = new Set(modifiers.toggle ? selection.rows : []);
    for (let row = from; row <= to; row++) {
      rows.add(row);
    }
    return { rows, anchor: selection.anchor };
  }
  if (modifiers.toggle) {
    const rows = new Set(selection.rows);
    if (rows.has(index)) {
      rows.delete(index);
    } else {
      rows.add(index);
    }
    return { rows, anchor: index };
  }
  return { rows: new Set([index]), anchor: index };
}

/** Selection for a right-click: keeps it if the row is part of it, else selects the row. */
export function selectForMenu(selection: Selection, index: number): Selection {
  return selection.rows.has(index) ? selection : { rows: new Set([index]), anchor: index };
}

/** Selected rows in list order. */
export function selectedRows(selection: Selection): number[] {
  return [...selection.rows].sort((a, b) => a - b);
}
