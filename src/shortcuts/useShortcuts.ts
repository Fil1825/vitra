import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import { useEnvironment } from '../stores/environment.ts';
import { useSettings } from '../stores/settings.ts';
import { ACTIONS, resolveBindings, type ActionId, type Bindings } from './actions.ts';
import { comboFromEvent, comboLabel, firesIn, focusKind } from './keys.ts';
import { RUN } from './run.ts';

/** While Settings records a new combination, shortcuts don't act. */
let recording = false;

export function setRecordingShortcut(on: boolean): void {
  recording = on;
}

export function useIsMac(): boolean {
  return useEnvironment((s) => s.platform === 'macos');
}

/** The shortcuts in effect in this window, following the settings. */
export function useBindings(): Bindings {
  const overrides = useSettings((s) => s.settings.shortcuts);
  const mac = useIsMac();
  return useMemo(() => resolveBindings(overrides, mac), [overrides, mac]);
}

/** An action's shortcut as people read it (`⌥⌘S`), or `null` if it has none. */
export function useShortcutLabel(action: ActionId): string | null {
  const { t } = useTranslation();
  const combo = useBindings().combos[action];
  const mac = useIsMac();
  return combo ? comboLabel(combo, mac, t('shortcuts.space')) : null;
}

/** A label with its shortcut after it, for tooltips: `Hide sidebar (⌥⌘S)`. */
export function withShortcut(label: string, shortcut: string | null): string {
  return shortcut ? `${label} (${shortcut})` : label;
}

/**
 * The window's keyboard shortcuts: one listener that turns a key press into an action. The
 * mini player's window runs only the actions marked for it. A key a focused control or text
 * field needs is left to it (`firesIn`), and so is one a component already handled.
 */
export function useShortcuts(scope: 'main' | 'mini'): void {
  const { actions } = useBindings();
  const mac = useIsMac();

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (recording || event.defaultPrevented || event.isComposing) return;
      const combo = comboFromEvent(event, mac);
      const action = combo ? actions.get(combo) : undefined;
      if (!combo || !action) return;
      const info = ACTIONS[action];
      if (scope === 'mini' && !info.mini) return;
      if (!firesIn(combo, focusKind(event.target))) return;
      // A held key acts once, unless the action is meant to go on (seeking, volume); it
      // still doesn't fall through to scrolling meanwhile.
      if ((event.repeat && !info.repeats) || RUN[action]() !== false) {
        event.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [actions, mac, scope]);
}
