import { normalizeCombo } from './keys.ts';

/**
 * What a shortcut can do. Ids are kept in the settings (`shortcuts`: id → combination, only
 * what the user changed), so they never change; labels are `shortcuts.actions.<id>`.
 */
export const ACTION_IDS = [
  'play-pause',
  'next',
  'previous',
  'seek-forward',
  'seek-back',
  'seek-forward-long',
  'seek-back-long',
  'volume-up',
  'volume-down',
  'mute',
  'shuffle',
  'repeat',
  'open-files',
  'open-link',
  'search',
  'back',
  'forward',
  'sidebar',
  'queue',
  'lyrics',
  'customize-layout',
  'mini-player',
  'settings',
] as const;

export type ActionId = (typeof ACTION_IDS)[number];

export type ActionGroup = 'playback' | 'app';

export interface ActionInfo {
  group: ActionGroup;
  /** The shortcut Vitra comes with, per platform (`null`: none until the user sets one). */
  mac: string | null;
  other: string | null;
  /** Held down, it keeps acting (seeking, volume); everything else acts once per press. */
  repeats?: boolean;
  /** Also in the mini player's window. */
  mini?: boolean;
}

/** The same shortcut on every platform. */
const both = (combo: string | null) => ({ mac: combo, other: combo });

export const ACTIONS: Record<ActionId, ActionInfo> = {
  'play-pause': { group: 'playback', ...both('Space'), mini: true },
  next: { group: 'playback', ...both('Mod+ArrowRight'), mini: true },
  previous: { group: 'playback', ...both('Mod+ArrowLeft'), mini: true },
  'seek-forward': { group: 'playback', ...both('ArrowRight'), repeats: true, mini: true },
  'seek-back': { group: 'playback', ...both('ArrowLeft'), repeats: true, mini: true },
  'seek-forward-long': {
    group: 'playback',
    ...both('Shift+ArrowRight'),
    repeats: true,
    mini: true,
  },
  'seek-back-long': { group: 'playback', ...both('Shift+ArrowLeft'), repeats: true, mini: true },
  'volume-up': { group: 'playback', ...both('Mod+ArrowUp'), repeats: true, mini: true },
  'volume-down': { group: 'playback', ...both('Mod+ArrowDown'), repeats: true, mini: true },
  mute: { group: 'playback', ...both('KeyM'), mini: true },
  shuffle: { group: 'playback', ...both(null), mini: true },
  repeat: { group: 'playback', ...both(null), mini: true },
  'open-files': { group: 'app', ...both('Mod+KeyO') },
  'open-link': { group: 'app', ...both('Mod+KeyU') },
  search: { group: 'app', ...both('Mod+KeyF') },
  // Like a browser: ⌘[ ⌘] on macOS, Alt+← Alt+→ elsewhere.
  back: { group: 'app', mac: 'Mod+BracketLeft', other: 'Alt+ArrowLeft' },
  forward: { group: 'app', mac: 'Mod+BracketRight', other: 'Alt+ArrowRight' },
  sidebar: { group: 'app', ...both('Mod+Alt+KeyS') },
  queue: { group: 'app', ...both(null) },
  lyrics: { group: 'app', ...both(null) },
  'customize-layout': { group: 'app', ...both(null) },
  // Apple Music's shortcut for its mini player.
  'mini-player': { group: 'app', ...both('Mod+Alt+KeyM'), mini: true },
  settings: { group: 'app', ...both('Mod+Comma') },
};

/** What the user changed: an action's combination, or `null` for none. */
export type ShortcutOverrides = Readonly<Record<string, string | null>>;

const isAction = (id: string): id is ActionId => (ACTION_IDS as readonly string[]).includes(id);

export function defaultCombo(action: ActionId, mac: boolean): string | null {
  const info = ACTIONS[action];
  return mac ? info.mac : info.other;
}

/** The shortcuts in effect: each action's combination and the action of each combination. */
export interface Bindings {
  combos: Record<ActionId, string | null>;
  actions: ReadonlyMap<string, ActionId>;
}

export function resolveBindings(overrides: ShortcutOverrides, mac: boolean): Bindings {
  const combos = {} as Record<ActionId, string | null>;
  const actions = new Map<string, ActionId>();
  for (const action of ACTION_IDS) {
    const chosen = Object.hasOwn(overrides, action) ? overrides[action] : defaultCombo(action, mac);
    const combo = chosen ? normalizeCombo(chosen) : null;
    // A hand-edited file may give two actions one combination: the first keeps it.
    if (combo && !actions.has(combo)) {
      actions.set(combo, action);
      combos[action] = combo;
    } else {
      combos[action] = null;
    }
  }
  return { combos, actions };
}

/** Overrides as small as they can be: only actions whose combination isn't the default. */
function compact(overrides: Record<string, string | null>, mac: boolean): ShortcutOverrides {
  return Object.fromEntries(
    Object.entries(overrides).filter(
      ([id, combo]) => isAction(id) && combo !== defaultCombo(id, mac),
    ),
  );
}

/**
 * Gives `action` the combination `combo` (`null`: none). An action that had it loses it;
 * those are returned so the user can be told.
 */
export function rebind(
  overrides: ShortcutOverrides,
  action: ActionId,
  combo: string | null,
  mac: boolean,
): { overrides: ShortcutOverrides; displaced: ActionId[] } {
  const normalized = combo === null ? null : normalizeCombo(combo);
  const { combos } = resolveBindings(overrides, mac);
  const next: Record<string, string | null> = { ...overrides };
  const displaced: ActionId[] = [];
  if (normalized !== null) {
    for (const other of ACTION_IDS) {
      if (other !== action && combos[other] === normalized) {
        next[other] = null;
        displaced.push(other);
      }
    }
  }
  next[action] = normalized;
  return { overrides: compact(next, mac), displaced };
}

/** Puts back Vitra's combination for `action` (taking it from another action if needed). */
export function resetAction(
  overrides: ShortcutOverrides,
  action: ActionId,
  mac: boolean,
): { overrides: ShortcutOverrides; displaced: ActionId[] } {
  return rebind(overrides, action, defaultCombo(action, mac), mac);
}
