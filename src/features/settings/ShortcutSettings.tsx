import { RotateCcw, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import {
  ACTION_IDS,
  ACTIONS,
  rebind,
  resetAction,
  type ActionGroup,
  type ActionId,
  type ShortcutOverrides,
} from '../../shortcuts/actions.ts';
import { comboFromEvent, comboLabel, isReserved } from '../../shortcuts/keys.ts';
import { setRecordingShortcut, useBindings, useIsMac } from '../../shortcuts/useShortcuts.ts';
import { useSettings } from '../../stores/settings.ts';
import styles from './SettingsPanel.module.css';

const GROUPS: readonly ActionGroup[] = ['playback', 'app'];

type Change = (overrides: ShortcutOverrides) => {
  overrides: ShortcutOverrides;
  displaced: ActionId[];
};

/**
 * Every action with its shortcut. Clicking a shortcut records the next key press as the new
 * one (Esc cancels); an action that had that combination loses it, and the row says so.
 */
export function ShortcutSettings() {
  const { t } = useTranslation();
  const mac = useIsMac();
  const overrides = useSettings((s) => s.settings.shortcuts);
  const update = useSettings((s) => s.update);
  const { combos } = useBindings();
  const [recording, setRecording] = useState<ActionId | null>(null);
  const [note, setNote] = useState<{ action: ActionId; text: string } | null>(null);
  const space = t('shortcuts.space');
  const label = (combo: string) => comboLabel(combo, mac, space);

  const apply = (action: ActionId, change: Change) => {
    let displaced: ActionId[] = [];
    update((s) => {
      const next = change(s.shortcuts);
      displaced = next.displaced;
      return { ...s, shortcuts: { ...next.overrides } };
    })
      .then(() => {
        const names = displaced.map((id) => t(`shortcuts.actions.${id}`)).join(', ');
        setNote(names ? { action, text: t('shortcuts.taken', { actions: names }) } : null);
      })
      .catch((err: unknown) => console.error('Changing a shortcut failed', err));
  };

  useEffect(() => {
    if (!recording) return;
    setRecordingShortcut(true);
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
        setNote({ action: recording, text: t('shortcuts.reserved', { combo: label(combo) }) });
        return;
      }
      apply(recording, (current) => rebind(current, recording, combo, mac));
      setRecording(null);
    };
    window.addEventListener('keydown', onKey, { capture: true });
    return () => {
      window.removeEventListener('keydown', onKey, { capture: true });
      setRecordingShortcut(false);
    };
    // `apply` and `label` are remade each render from the values listed here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording, mac]);

  return (
    <>
      <p className={styles.hint}>{t('shortcuts.hint')}</p>
      {GROUPS.map((group) => (
        <div key={group} className={styles.shortcutGroup}>
          <h4 className={styles.shortcutGroupTitle}>{t(`shortcuts.groups.${group}`)}</h4>
          {ACTION_IDS.filter((id) => ACTIONS[id].group === group).map((id) => {
            const name = t(`shortcuts.actions.${id}`);
            const combo = combos[id];
            const changed = Object.hasOwn(overrides, id);
            return (
              <div key={id} className={styles.shortcutRow}>
                <span className={styles.shortcutName}>{name}</span>
                <button
                  type="button"
                  className={styles.keycap}
                  data-recording={recording === id || undefined}
                  data-empty={(!combo && recording !== id) || undefined}
                  aria-label={t('shortcuts.change', { action: name })}
                  onClick={() => {
                    setNote(null);
                    setRecording(recording === id ? null : id);
                  }}
                  onBlur={() => setRecording((current) => (current === id ? null : current))}
                >
                  {recording === id
                    ? t('shortcuts.record')
                    : combo
                      ? label(combo)
                      : t('shortcuts.none')}
                </button>
                <span className={styles.shortcutButtons}>
                  {combo && (
                    <IconButton
                      label={t('shortcuts.remove')}
                      onClick={() => apply(id, (current) => rebind(current, id, null, mac))}
                    >
                      <X size={13} />
                    </IconButton>
                  )}
                  {changed && (
                    <IconButton
                      label={t('shortcuts.reset')}
                      onClick={() => apply(id, (current) => resetAction(current, id, mac))}
                    >
                      <RotateCcw size={13} />
                    </IconButton>
                  )}
                </span>
                {note?.action === id && (
                  <span className={styles.shortcutNote} role="status">
                    {note.text}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      ))}
      {Object.keys(overrides).length > 0 && (
        <div className={styles.buttonRow}>
          <button
            type="button"
            className={styles.textButton}
            onClick={() => {
              setNote(null);
              void update((s) => ({ ...s, shortcuts: {} })).catch((err: unknown) =>
                console.error('Resetting shortcuts failed', err),
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
