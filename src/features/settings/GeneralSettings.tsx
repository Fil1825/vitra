import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import type { Language } from '../../lib/settings.ts';
import { system } from '../../lib/system.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useSettings } from '../../stores/settings.ts';
import { useUi } from '../../stores/ui.ts';
import { Group, Row } from './Row.tsx';
import styles from './SettingsPanel.module.css';

/** Settings → General: what concerns the whole app. */
export function GeneralSettings() {
  const { t } = useTranslation();
  const language = useSettings((s) => s.settings.language);
  const trayIcon = useSettings((s) => s.settings.system.trayIcon);
  const update = useSettings((s) => s.update);
  const platform = useEnvironment((s) => s.platform);
  const showSettings = useUi((s) => s.showSettings);
  const [trayAvailable, setTrayAvailable] = useState(true);

  useEffect(() => {
    system
      .trayAvailable()
      .then(setTrayAvailable)
      .catch((err: unknown) => console.error('Asking about the tray failed', err));
  }, []);

  const trayLabel =
    platform === 'macos'
      ? t('settings.general.trayIcon.macos')
      : platform === 'windows'
        ? t('settings.general.trayIcon.windows')
        : t('settings.general.trayIcon.linux');

  return (
    <>
      <Group>
        <Row label={t('settings.language')}>
          <SegmentedControl<Language>
            label={t('settings.language')}
            value={language}
            options={[
              { value: 'system', label: t('settings.languages.system') },
              { value: 'en', label: t('settings.languages.en') },
              { value: 'ru', label: t('settings.languages.ru') },
            ]}
            onChange={(next) => void update((current) => ({ ...current, language: next }))}
          />
        </Row>
      </Group>
      <Group>
        <Row
          label={trayLabel}
          hint={
            trayAvailable ? t('settings.general.trayHint') : t('settings.general.trayUnavailable')
          }
          layout="inline"
        >
          <Switch
            label={trayLabel}
            checked={trayIcon && trayAvailable}
            disabled={!trayAvailable}
            onChange={(on) =>
              void update((current) => ({
                ...current,
                system: { ...current.system, trayIcon: on },
              }))
            }
          />
        </Row>
        <Row label={t('settings.general.globalShortcuts')} hint={t('settings.general.globalHint')}>
          <div className={styles.buttonRow}>
            <button
              type="button"
              className={styles.textButton}
              onClick={() => showSettings(true, 'shortcuts')}
            >
              {t('settings.general.openShortcuts')}
            </button>
          </div>
        </Row>
      </Group>
    </>
  );
}
