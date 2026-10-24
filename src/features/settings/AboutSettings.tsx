import { getVersion } from '@tauri-apps/api/app';
import { isTauri } from '@tauri-apps/api/core';
import { Code2, HandHeart } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import logo from '../../../assets/brand/symbol.svg';
import { openInBrowser } from '../../lib/browser.ts';
import { DONATE_URL, REPOSITORY_URL, authorNote } from './about.ts';
import { Diagnostics } from './Diagnostics.tsx';
import { Group } from './Row.tsx';
import styles from './SettingsPanel.module.css';

const open = (url: string) => () =>
  void openInBrowser(url).catch((err: unknown) => console.error('Opening a page failed', err));

/** What Vitra stands on, credited by name. */
const CREDITS = ['mpv', 'yt-dlp', 'FFmpeg', 'Deno', 'Tauri', 'Outfit · Onest (SIL OFL)'];

/** Settings → About: the app, its version, the author's note, support, credits. */
export function AboutSettings() {
  const { t, i18n } = useTranslation();
  const [version, setVersion] = useState<string | null>(null);
  const note = authorNote(i18n.language);

  useEffect(() => {
    if (!isTauri()) return;
    getVersion()
      .then(setVersion)
      .catch((err: unknown) => console.error('Reading the version failed', err));
  }, []);

  return (
    <>
      <div className={styles.about}>
        <img className={styles.aboutLogo} src={logo} alt="" width={72} height={72} />
        <div className={styles.aboutText}>
          <h4 className={styles.aboutName}>Vitra</h4>
          {version && <span className={styles.hint}>{t('about.version', { version })}</span>}
          <span className={styles.aboutTagline}>{t('about.tagline')}</span>
        </div>
      </div>

      {note && (
        <Group>
          <p className={styles.aboutNote}>{note}</p>
        </Group>
      )}

      <div className={styles.buttonRow}>
        {DONATE_URL && (
          <button type="button" className={styles.accentButton} onClick={open(DONATE_URL)}>
            <HandHeart size={15} aria-hidden="true" />
            {t('about.support')}
          </button>
        )}
        <button type="button" className={styles.textButton} onClick={open(REPOSITORY_URL)}>
          <Code2 size={14} aria-hidden="true" />
          {t('about.source')}
        </button>
      </div>

      <Group title={t('about.license')}>
        <p className={styles.hint}>{t('about.licenseText')}</p>
        <p className={styles.hint}>
          {t('about.credits')} {CREDITS.join(', ')}.
        </p>
      </Group>

      <Group title={t('settings.diagnostics')}>
        <Diagnostics />
      </Group>
    </>
  );
}
