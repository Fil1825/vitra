import { PictureInPicture2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { Slider } from '../../components/ui/Slider.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import { MINI_MODES, MINI_SIZES, mini } from '../../lib/mini.ts';
import { MINI_OPACITY_MIN, type MiniPlayerSettings as Values } from '../../lib/settings.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import styles from './SettingsPanel.module.css';

/** The mini player's mode and window: the same settings as its own context menu, and more. */
export function MiniPlayerSettings() {
  const { t } = useTranslation();
  const values = useSettings((s) => s.settings.miniPlayer);
  const update = useSettings((s) => s.update);
  const platform = useEnvironment((s) => s.platform);
  const mac = platform === 'macos';
  const set = (change: Partial<Values>) =>
    void update((current) => ({ ...current, miniPlayer: { ...current.miniPlayer, ...change } }));

  const toggle = (key: keyof Values, label: string, hint?: string, disabled?: boolean) => (
    <div className={styles.inlineRow}>
      <div className={styles.rowText}>
        <span>{label}</span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </div>
      <Switch
        label={label}
        checked={values[key] === true}
        disabled={disabled}
        onChange={(checked) => set({ [key]: checked })}
      />
    </div>
  );

  return (
    <>
      <div className={styles.buttonRow}>
        <button
          type="button"
          className={styles.addFolder}
          onClick={() => void mini.show().catch((err: unknown) => console.error(err))}
        >
          <PictureInPicture2 size={15} aria-hidden="true" />
          {t('settings.mini.open', { shortcut: mac ? '⌥⌘M' : 'Ctrl+Alt+M' })}
        </button>
      </div>

      <div className={styles.row}>
        <div className={styles.rowText}>
          <span>{t('settings.mini.mode')}</span>
        </div>
        <SegmentedControl
          label={t('settings.mini.mode')}
          value={values.mode}
          options={MINI_MODES.map((mode) => ({ value: mode, label: t(`mini.modes.${mode}`) }))}
          onChange={(mode) => set({ mode })}
        />
      </div>

      <div className={styles.row}>
        <div className={styles.rowText}>
          <span>{t('mini.size')}</span>
          <span className={styles.hint}>{t('settings.mini.sizeHint')}</span>
        </div>
        <SegmentedControl
          label={t('mini.size')}
          value={values.size}
          options={MINI_SIZES.map((size) => ({ value: size, label: t(`mini.sizes.${size}`) }))}
          onChange={(size) => set({ size })}
        />
      </div>

      <div className={styles.row}>
        <div className={styles.rowText}>
          <span>
            {t('mini.opacity')}: {Math.round(values.opacity * 100)}%
          </span>
          <span className={styles.hint}>
            {platform === 'linux'
              ? t('settings.mini.opacityLinux')
              : t('settings.mini.opacityHint')}
          </span>
        </div>
        <Slider
          label={t('mini.opacity')}
          value={values.opacity}
          min={MINI_OPACITY_MIN}
          max={1}
          step={0.05}
          valueText={`${Math.round(values.opacity * 100)}%`}
          onChange={(opacity) => set({ opacity })}
        />
      </div>

      {toggle('alwaysOnTop', t('mini.alwaysOnTop'))}
      {mac &&
        toggle(
          'overFullScreen',
          t('settings.mini.overFullScreen'),
          t('settings.mini.overFullScreenHint'),
          !values.alwaysOnTop,
        )}
      {toggle(
        'clickThrough',
        t('mini.clickThrough'),
        platform === 'linux'
          ? t('settings.mini.clickThroughLinux')
          : t('settings.mini.clickThroughHint', { key: mac ? '⌥' : 'Alt' }),
      )}
      {toggle('snapToEdges', t('settings.mini.snapToEdges'))}
      {toggle('rememberPosition', t('settings.mini.rememberPosition'))}
      {toggle(
        'hideMainWindow',
        t('settings.mini.hideMainWindow'),
        t('settings.mini.hideMainWindowHint'),
      )}
    </>
  );
}
