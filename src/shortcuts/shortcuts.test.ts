import { describe, expect, it } from 'vitest';

import {
  ACTION_IDS,
  ACTIONS,
  globalAllowed,
  isGlobalAction,
  rebind,
  rebindGlobal,
  rebindInWindow,
  resetAction,
  resolveBindings,
} from './actions.ts';
import {
  comboFromEvent,
  comboLabel,
  firesIn,
  isReserved,
  keyLabel,
  normalizeCombo,
  parseCombo,
} from './keys.ts';

const press = (code: string, mods: Partial<KeyboardEventInit> = {}) =>
  ({
    code,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...mods,
  }) as KeyboardEvent;

describe('combinations', () => {
  it('read a key press by the key’s place, with modifiers in one order', () => {
    expect(comboFromEvent(press('KeyS', { metaKey: true, altKey: true }), true)).toBe(
      'Mod+Alt+KeyS',
    );
    expect(comboFromEvent(press('KeyS', { ctrlKey: true, altKey: true }), false)).toBe(
      'Mod+Alt+KeyS',
    );
    // Control is its own modifier on macOS only.
    expect(comboFromEvent(press('KeyS', { ctrlKey: true }), true)).toBe('Ctrl+KeyS');
    expect(comboFromEvent(press('Space', { shiftKey: true }), true)).toBe('Shift+Space');
  });

  it('wait while only modifiers are down, and leave the Windows key alone', () => {
    expect(comboFromEvent(press('ShiftLeft', { shiftKey: true }), true)).toBeNull();
    expect(comboFromEvent(press('MetaLeft', { metaKey: true }), true)).toBeNull();
    expect(comboFromEvent(press('KeyE', { metaKey: true }), false)).toBeNull();
    expect(comboFromEvent(press(''), false)).toBeNull();
  });

  it('are normalized, and nonsense is refused', () => {
    expect(normalizeCombo('Shift+Alt+Mod+KeyK')).toBe('Mod+Alt+Shift+KeyK');
    expect(normalizeCombo('KeyK')).toBe('KeyK');
    for (const bad of ['', 'Mod+', 'Mod+Mod+KeyK', 'Hyper+KeyK', 'Mod+ShiftLeft', 'Mod+<b>']) {
      expect(parseCombo(bad)).toBeNull();
    }
  });

  it('read as keycaps', () => {
    expect(comboLabel('Mod+Alt+KeyS', true)).toBe('⌥⌘S');
    expect(comboLabel('Mod+Alt+KeyS', false)).toBe('Ctrl+Alt+S');
    expect(comboLabel('Mod+Ctrl+Shift+Digit3', true)).toBe('⌃⇧⌘3');
    expect(comboLabel('Mod+Comma', false)).toBe('Ctrl+,');
    expect(comboLabel('Space', true, 'Пробел')).toBe('Пробел');
    expect(comboLabel('Shift+ArrowLeft', false)).toBe('Shift+←');
    expect(keyLabel('Backspace', true)).toBe('⌫');
    expect(keyLabel('Numpad5', false)).toBe('Num 5');
    expect(keyLabel('F5', false)).toBe('F5');
  });

  it('the system’s and the menu’s are reserved', () => {
    expect(isReserved('Mod+KeyQ', true)).toBe(true);
    expect(isReserved('Mod+KeyQ', false)).toBe(false);
    expect(isReserved('Mod+KeyC', false)).toBe(true);
    expect(isReserved('Escape', true)).toBe(true);
    expect(isReserved('Mod+KeyK', true)).toBe(false);
  });
});

describe('where shortcuts act', () => {
  it('plain keys only with nothing focused', () => {
    expect(firesIn('Space', 'none')).toBe(true);
    expect(firesIn('Space', 'control')).toBe(false);
    expect(firesIn('Shift+ArrowLeft', 'control')).toBe(false);
    expect(firesIn('KeyM', 'text')).toBe(false);
  });

  it('chords in controls, and in text fields unless they edit text', () => {
    expect(firesIn('Mod+ArrowRight', 'control')).toBe(true);
    expect(firesIn('Mod+ArrowRight', 'text')).toBe(false);
    expect(firesIn('Alt+ArrowLeft', 'text')).toBe(false);
    expect(firesIn('Mod+KeyZ', 'text')).toBe(false);
    expect(firesIn('Mod+KeyF', 'text')).toBe(true);
    expect(firesIn('Mod+Alt+KeyS', 'text')).toBe(true);
  });
});

describe('bindings', () => {
  it('start as Vitra’s, different per platform where it matters', () => {
    const mac = resolveBindings({}, true);
    const other = resolveBindings({}, false);
    expect(mac.combos.back).toBe('Mod+BracketLeft');
    expect(other.combos.back).toBe('Alt+ArrowLeft');
    expect(mac.actions.get('Space')).toBe('play-pause');
    expect(mac.combos.queue).toBeNull();
  });

  it('give every action a different combination', () => {
    for (const mac of [true, false]) {
      const combos = ACTION_IDS.map((id) => (mac ? ACTIONS[id].mac : ACTIONS[id].other));
      const set = combos.filter((c) => c !== null);
      expect(new Set(set).size).toBe(set.length);
      for (const combo of set) {
        expect(normalizeCombo(combo)).toBe(combo);
        expect(isReserved(combo, mac)).toBe(false);
      }
    }
  });

  it('follow the user’s changes, and a combination given twice stays with the first', () => {
    const { combos, actions } = resolveBindings(
      { mute: null, queue: 'Mod+Alt+KeyQ', lyrics: 'Mod+Alt+KeyQ', nope: 'KeyX' },
      true,
    );
    expect(combos.mute).toBeNull();
    expect(actions.has('KeyM')).toBe(false);
    expect(combos.queue).toBe('Mod+Alt+KeyQ');
    expect(combos.lyrics).toBeNull();
    expect(actions.has('KeyX')).toBe(false);
  });

  it('rebinding takes the combination from the action that had it', () => {
    const { overrides, displaced } = rebind({}, 'queue', 'KeyM', true);
    expect(displaced).toEqual(['mute']);
    expect(overrides).toEqual({ queue: 'KeyM', mute: null });
    const back = resetAction(overrides, 'mute', true);
    expect(back.displaced).toEqual(['queue']);
    // Both back to Vitra's: nothing left to keep.
    expect(back.overrides).toEqual({});
  });

  it('removing a shortcut is kept; choosing the default again is not', () => {
    expect(rebind({}, 'mute', null, false).overrides).toEqual({ mute: null });
    expect(rebind({ mute: 'KeyK' }, 'mute', 'KeyM', false).overrides).toEqual({});
  });
});

describe('shortcuts in every app', () => {
  it('are for playback and the mini player', () => {
    expect(isGlobalAction('play-pause')).toBe(true);
    expect(isGlobalAction('mini-player')).toBe(true);
    expect(isGlobalAction('search')).toBe(false);
  });

  it('need a modifier that doesn’t type, unless the key types nothing', () => {
    expect(globalAllowed('Mod+Alt+KeyP')).toBe(true);
    expect(globalAllowed('Ctrl+KeyP')).toBe(true);
    expect(globalAllowed('KeyP')).toBe(false);
    expect(globalAllowed('Shift+KeyP')).toBe(false);
    expect(globalAllowed('Space')).toBe(false);
    expect(globalAllowed('F13')).toBe(true);
    expect(globalAllowed('Shift+F8')).toBe(true);
  });

  it('take the combination from other actions, in every app and in the window', () => {
    const globals = { next: 'Mod+Alt+KeyN' };
    const taken = rebindGlobal({}, globals, 'previous', 'Mod+Alt+KeyN', true);
    expect(taken.globals).toEqual({ previous: 'Mod+Alt+KeyN' });
    expect(taken.displaced).toEqual(['next']);
    // ⌥⌘S is the sidebar's in the window: the system would catch it first.
    const window = rebindGlobal({}, {}, 'mini-player', 'Mod+Alt+KeyS', true);
    expect(window.globals).toEqual({ 'mini-player': 'Mod+Alt+KeyS' });
    expect(window.overrides).toEqual({ sidebar: null });
    expect(window.displaced).toEqual(['sidebar']);
    // The action's own window shortcut may be the same.
    const own = rebindGlobal({}, {}, 'next', 'Mod+ArrowRight', true);
    expect(own.overrides).toEqual({});
    expect(own.displaced).toEqual([]);
    expect(rebindGlobal({}, own.globals, 'next', null, true).globals).toEqual({});
  });

  it('give way to a window shortcut set later for another action', () => {
    const result = rebindInWindow({}, { next: 'Mod+KeyK' }, 'mute', 'Mod+KeyK', true);
    expect(result.overrides).toEqual({ mute: 'Mod+KeyK' });
    expect(result.globals).toEqual({});
    expect(result.displaced).toEqual(['next']);
    const same = rebindInWindow({}, { mute: 'Mod+KeyK' }, 'mute', 'Mod+KeyK', true);
    expect(same.globals).toEqual({ mute: 'Mod+KeyK' });
  });
});
