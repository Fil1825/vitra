/**
 * Key combinations as text: modifiers in a fixed order, then the key's `KeyboardEvent.code`
 * (`Mod+Alt+KeyS`). `Mod` is ⌘ on macOS and Ctrl elsewhere; `Ctrl` is the Control key on
 * macOS only. The code names the key's place, not its letter, so a shortcut works whatever
 * the keyboard layout (Russian, or ⌥ turning S into ß on macOS).
 */

const MODIFIERS = ['Mod', 'Ctrl', 'Alt', 'Shift'] as const;
type Modifier = (typeof MODIFIERS)[number];

/** Keys that are modifiers themselves: pressing one alone is not a shortcut yet. */
const MODIFIER_CODES = new Set([
  'MetaLeft',
  'MetaRight',
  'OSLeft',
  'OSRight',
  'ControlLeft',
  'ControlRight',
  'AltLeft',
  'AltRight',
  'ShiftLeft',
  'ShiftRight',
  'CapsLock',
  'Fn',
  'FnLock',
]);

const CODE = /^[A-Za-z][A-Za-z0-9]{0,31}$/;

/** The parts of a combination; `null` if the text isn't one. */
export function parseCombo(
  combo: string,
): { modifiers: ReadonlySet<Modifier>; code: string } | null {
  const parts = combo.split('+');
  const code = parts.pop();
  if (!code || !CODE.test(code) || MODIFIER_CODES.has(code)) return null;
  const modifiers = new Set<Modifier>();
  for (const part of parts) {
    const modifier = MODIFIERS.find((m) => m === part);
    if (!modifier || modifiers.has(modifier)) return null;
    modifiers.add(modifier);
  }
  return { modifiers, code };
}

/** The same combination in the canonical order, or `null`. */
export function normalizeCombo(combo: string): string | null {
  const parsed = parseCombo(combo);
  if (!parsed) return null;
  return [...MODIFIERS.filter((m) => parsed.modifiers.has(m)), parsed.code].join('+');
}

/**
 * The combination a key press makes, or `null` while only modifiers are down (or for keys
 * that can't be told apart, and the Windows key, which belongs to the system).
 */
export function comboFromEvent(event: KeyboardEvent, mac: boolean): string | null {
  const { code } = event;
  if (!code || code === 'Unidentified' || MODIFIER_CODES.has(code)) return null;
  if (!mac && event.metaKey) return null;
  const parts: string[] = [];
  if (mac ? event.metaKey : event.ctrlKey) parts.push('Mod');
  if (mac && event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  parts.push(code);
  return normalizeCombo(parts.join('+'));
}

/** Whether a combination has ⌘/Ctrl/⌃/⌥ in it (Shift alone still types). */
export function isChord(combo: string): boolean {
  const parsed = parseCombo(combo);
  return parsed !== null && ['Mod', 'Ctrl', 'Alt'].some((m) => parsed.modifiers.has(m as Modifier));
}

const PUNCTUATION: Record<string, string> = {
  Comma: ',',
  Period: '.',
  Slash: '/',
  Backslash: '\\',
  Semicolon: ';',
  Quote: "'",
  BracketLeft: '[',
  BracketRight: ']',
  Minus: '-',
  Equal: '=',
  Backquote: '`',
  IntlBackslash: '§',
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
};

const NAMED: Record<string, [mac: string, other: string]> = {
  Enter: ['↩', 'Enter'],
  NumpadEnter: ['⌅', 'Enter'],
  Escape: ['Esc', 'Esc'],
  Backspace: ['⌫', 'Backspace'],
  Delete: ['⌦', 'Delete'],
  Tab: ['⇥', 'Tab'],
  Home: ['↖', 'Home'],
  End: ['↘', 'End'],
  PageUp: ['⇞', 'PgUp'],
  PageDown: ['⇟', 'PgDn'],
  Insert: ['Ins', 'Ins'],
};

/** How a key is written on a keycap (the US layout's, as the code names it). */
export function keyLabel(code: string, mac: boolean, space = 'Space'): string {
  if (code === 'Space') return space;
  const named = NAMED[code];
  if (named) return mac ? named[0] : named[1];
  const letter = /^Key([A-Z])$/.exec(code) ?? /^Digit(\d)$/.exec(code);
  if (letter) return letter[1] ?? code;
  const numpad = /^Numpad(.+)$/.exec(code);
  if (numpad) return `Num ${numpad[1]}`;
  return PUNCTUATION[code] ?? code;
}

/** A combination as people read it: `⌥⌘S` on macOS, `Ctrl+Alt+S` elsewhere. */
export function comboLabel(combo: string, mac: boolean, space?: string): string {
  const parsed = parseCombo(combo);
  if (!parsed) return combo;
  const key = keyLabel(parsed.code, mac, space);
  const has = (m: Modifier) => parsed.modifiers.has(m);
  if (mac) {
    // Apple's order: ⌃ ⌥ ⇧ ⌘.
    return `${has('Ctrl') ? '⌃' : ''}${has('Alt') ? '⌥' : ''}${has('Shift') ? '⇧' : ''}${has('Mod') ? '⌘' : ''}${key}`;
  }
  return [has('Mod') && 'Ctrl', has('Alt') && 'Alt', has('Shift') && 'Shift', key]
    .filter(Boolean)
    .join('+');
}

/**
 * Combinations a shortcut may not take: the system's and the menu's (they never reach the
 * page), editing in text fields, and the keys that move focus or close dialogs.
 */
export function isReserved(combo: string, mac: boolean): boolean {
  const common = ['Escape', 'Tab', 'Shift+Tab', 'Enter', 'Mod+KeyA', 'Mod+KeyC', 'Mod+KeyV'];
  const own = mac
    ? ['Mod+KeyQ', 'Mod+KeyW', 'Mod+KeyH', 'Mod+Alt+KeyH', 'Mod+KeyM', 'Mod+KeyX', 'Mod+KeyZ']
    : ['Mod+KeyX', 'Mod+KeyZ', 'Mod+KeyY', 'Alt+F4', 'Alt+Tab'];
  const more = mac ? ['Mod+Shift+KeyZ', 'Mod+Tab', 'Mod+Space', 'Mod+Ctrl+KeyF'] : [];
  return [...common, ...own, ...more].includes(combo);
}

/** Where the keyboard focus is, for deciding which shortcuts may act. */
export type FocusKind = 'text' | 'control' | 'none';

const TEXT_INPUTS = new Set(['', 'text', 'search', 'url', 'email', 'number', 'password', 'tel']);

export function focusKind(target: EventTarget | null): FocusKind {
  if (!(target instanceof HTMLElement)) return 'none';
  if (target.isContentEditable || target instanceof HTMLTextAreaElement) return 'text';
  if (target instanceof HTMLInputElement) {
    return TEXT_INPUTS.has(target.type) ? 'text' : 'control';
  }
  const control =
    'button, select, a[href], [role="menu"], [role="slider"], [role="radio"], [role="tab"]';
  return target.closest(control) ? 'control' : 'none';
}

/** Keys a text field uses with ⌘/Ctrl/⌥ to move or edit. */
const EDITING = /^(Arrow\w+|Backspace|Delete|Home|End|Page\w+|Key[ACVXYZ])$/;

/**
 * Whether a shortcut may act with the focus there: plain keys (Space, arrows, letters) belong
 * to a focused control or field, and in a field the editing chords too.
 */
export function firesIn(combo: string, focus: FocusKind): boolean {
  if (focus === 'none') return true;
  if (!isChord(combo)) return false;
  if (focus === 'control') return true;
  const code = parseCombo(combo)?.code ?? '';
  return !EDITING.test(code);
}
