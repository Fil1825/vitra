import { RotateCcw, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { system } from '../../lib/system.ts';
import {
  ACTION_IDS,
  ACTIONS,
  defaultCombo,
  globalAllowed,
  isGlobalAction,
  rebindGlobal,
  rebindInWindow,
  type ActionGroup,
  type ActionId,
  type GlobalShortcuts,
  type Rebound,
  type ShortcutOverrides,
} from '../../shortcuts/actions.ts';
import { comboFromEvent, comboLabel, isReserved } from '../../shortcuts/keys.ts';
import { setRecordingShortcut, useBindings, useIsMac } from '../../shortcuts/useShortcuts.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import styles from './SettingsPanel.module.css';

const GROUPS: readonly ActionGroup[] = ['playback', 'app'];

type Change = (overrides: ShortcutOverrides, globals: GlobalShortcuts) => Rebound;

/** A shortcut being recorded: in the window, or in every app. */
interface Target {
  action: ActionId;
  global: boolean;
}

/**
 * Every action with its shortcut in the window and, for playback and the mini player, one
 * that works in every app. Clicking a shortcut records the next key press as the new one
 * (Esc cancels); an action that had that combination loses it, and the row says so.
 */
export function ShortcutSettings() {
  const { t } = useTranslation();
  const mac = useIsMac();
  const platform = useEnvironment((s) => s.platform);
  const overrides = useSettings((s) => s.settings.shortcuts);
  const globals = useSettings((s) => s.settings.globalShortcuts);
  const update = useSettings((s) => s.update);
  const { combos } = useBindings();
  const [recording, setRecording] = useState<Target | null>(null);
  const [note, setNote] = useState<{ action: ActionId; text: string } | null>(null);
  const [failed, setFailed] = useState<readonly string[]>([]);
  const space = t('shortcuts.space');
  const label = (combo: string) => comboLabel(combo, mac, space);
  const modifiers = mac ? t('shortcuts.modifiers.macos') : t('shortcuts.modifiers.other');

  // The system refuses a combination another app registered first.
  useEffect(() => {
    system
      .globalShortcutsFailed()
      .then(setFailed)
      .catch((err: unknown) => console.error('Asking about shortcuts failed', err));
  }, [globals]);

  const apply = (action: ActionId, change: Change) => {
    let displaced: ActionId[] = [];
    update((s) => {
      const next = change(s.shortcuts, s.globalShortcuts);
      displaced = next.displaced;
      return { ...s, shortcuts: { ...next.overrides }, globalShortcuts: { ...next.globals } };
    })
      .then(() => {
        const names = displaced.map((id) => t(`shortcuts.actions.${id}`)).join(', ');
        setNote(names ? { action, text: t('shortcuts.taken', { actions: names }) } : null);
      })
      .catch((err: unknown) => console.error('Changing a shortcut failed', err));
  };

  const setInWindow = (action: ActionId, combo: string | null) =>
    apply(action, (current, currentGlobals) =>
      rebindInWindow(current, currentGlobals, action, combo, mac),
    );
  const setGlobal = (action: ActionId, combo: string | null) =>
    apply(action, (current, currentGlobals) =>
      rebindGlobal(current, currentGlobals, action, combo, mac),
    );

  useEffect(() => {
    if (!recording) return;
    setRecordingShortcut(true);
    const { action, global } = recording;
    // Before anything else sees the keys: the app's shortcuts, Escape closing Settings.
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      const plain = !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
      if (event.key === 'Escape' && plain) {
        setRecording(null);
        return;
      }
      const combo = comboFromEvent(event, mac);
      if (!combo) return;
      if (isReserved(combo, mac)) {
        setNote({ action, text: t('shortcuts.reserved', { combo: label(combo) }) });
        return;
      }
      if (global && !globalAllowed(combo)) {
        setNote({ action, text: t('shortcuts.needsModifier', { combo: label(combo), modifiers }) });
        return;
      }
      if (global) setGlobal(action, combo);
      else setInWindow(action, combo);
      setRecording(null);
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => {
      window.removeEventListener('keydown', onKey, { capture: true });
      setRecordingShortcut(false);
    };
    // The helpers are remade each render from the values listed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording, mac]);

  const keycap = (action: ActionId, global: boolean, combo: string | null, name: string) => {
    const active = recording?.action === action && recording.global === global;
    const refused = global && failed.includes(action);
    return (
      <button
        type="button"
        className={styles.keycap}
        data-recording={active || undefined}
        data-empty={(!combo && !active) || undefined}
        data-failed={refused || undefined}
        title={refused ? t('shortcuts.globalFailed') : undefined}
        aria-label={
          global
            ? t('shortcuts.changeGlobal', { action: name })
            : t('shortcuts.change', { action: name })
        }
        onClick={() => {
          setNote(null);
          setRecording(active ? null : { action, global });
        }}
        onBlur={() =>
          setRecording((current) =>
            current?.action === action && current.global === global ? null : current,
          )
        }
      >
        {active ? t('shortcuts.record') : combo ? label(combo) : t('shortcuts.none')}
      </button>
    );
  };

  const changedAny = Object.keys(overrides).length > 0 || Object.keys(globals).length > 0;

  return (
    <>
      <p className={styles.hint}>{t('shortcuts.hint')}</p>
      <p className={styles.hint}>
        {t('shortcuts.globalHint', { modifiers })}
        {platform === 'linux' && ` ${t('shortcuts.waylandHint')}`}
      </p>
      <div className={styles.shortcutTable}>
        {GROUPS.map((group) => (
          <div key={group} className={styles.shortcutGroup}>
            <div className={styles.shortcutRow}>
              <h4 className={styles.shortcutGroupTitle}>{t(`shortcuts.groups.${group}`)}</h4>
              <span className={styles.shortcutColumn}>{t('shortcuts.inWindow')}</span>
              <span />
              <span className={styles.shortcutColumn}>{t('shortcuts.everywhere')}</span>
              <span />
            </div>
            {ACTION_IDS.filter((id) => ACTIONS[id].group === group).map((id) => {
              const name = t(`shortcuts.actions.${id}`);
              const combo = combos[id];
              const changed = Object.hasOwn(overrides, id);
              const global = globals[id] ?? null;
              return (
                <div key={id} className={styles.shortcutRow}>
                  <span className={styles.shortcutName}>{name}</span>
                  {keycap(id, false, combo, name)}
                  <span className={styles.shortcutButtons}>
                    {combo && (
                      <IconButton
                        label={t('shortcuts.remove')}
                        onClick={() => setInWindow(id, null)}
                      >
                        <X size={13} />
                      </IconButton>
                    )}
                    {changed && (
                      <IconButton
                        label={t('shortcuts.reset')}
                        onClick={() => setInWindow(id, defaultCombo(id, mac))}
                      >
                        <RotateCcw size={13} />
                      </IconButton>
                    )}
                  </span>
                  {isGlobalAction(id) ? keycap(id, true, global, name) : <span />}
                  <span className={styles.shortcutButtons}>
                    {global && (
                      <IconButton label={t('shortcuts.remove')} onClick={() => setGlobal(id, null)}>
                        <X size={13} />
                      </IconButton>
                    )}
                  </span>
                  {note?.action === id ? (
                    <span className={styles.shortcutNote} role="status">
                      {note.text}
                    </span>
                  ) : (
                    global &&
                    failed.includes(id) && (
                      <span className={styles.shortcutNote} role="status">
                        {t('shortcuts.globalFailed')}
                      </span>
                    )
                  )}
                </div>
              );
            })}
          </div>
        ))}
      </div>
      {changedAny && (
        <div className={styles.buttonRow}>
          <button
            type="button"
            className={styles.textButton}
            onClick={() => {
              setNote(null);
              void update((s) => ({ ...s, shortcuts: {}, globalShortcuts: {} })).catch(
                (err: unknown) => console.error('Resetting shortcuts failed', err),
              );
            }}
          >
            <RotateCcw size={14} aria-hidden="true" />
            {t('shortcuts.resetAll')}
          </button>
        </div>
      )}
    </>
  );
}
