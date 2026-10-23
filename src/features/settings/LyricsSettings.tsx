import { useTranslation } from 'react-i18next';

import { Switch } from '../../components/ui/Switch.tsx';
import type { LyricsSettings as Values } from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import styles from './SettingsPanel.module.css';

/** Online lookups and translations. Text size and sync offset live in the lyrics view. */
export function LyricsSettings() {
  const { t } = useTranslation();
  const lyrics = useSettings((s) => s.settings.lyrics);
  const update = useSettings((s) => s.update);
  const set = (change: Partial<Values>) =>
    void update((current) => ({ ...current, lyrics: { ...current.lyrics, ...change } }));

  return (
    <>
      <div className={styles.inlineRow}>
        <div className={styles.rowText}>
          <span>{t('settings.lyrics.online')}</span>
          <span className={styles.hint}>{t('settings.lyrics.onlineHint')}</span>
        </div>
        <Switch
          label={t('settings.lyrics.online')}
          checked={lyrics.online}
          onChange={(online) => set({ online })}
        />
      </div>
      <div className={styles.inlineRow}>
        <div className={styles.rowText}>
          <span>{t('settings.lyrics.translation')}</span>
          <span className={styles.hint}>{t('settings.lyrics.translationHint')}</span>
        </div>
        <Switch
          label={t('settings.lyrics.translation')}
          checked={lyrics.showTranslation}
          onChange={(showTranslation) => set({ showTranslation })}
        />
      </div>
    </>
  );
}
