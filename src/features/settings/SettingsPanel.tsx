import { Search, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useMemo, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { useUi } from '../../stores/ui.ts';
import { AboutSettings } from './AboutSettings.tsx';
import { AppearanceSettings } from './AppearanceSettings.tsx';
import { DownloadSettings } from './DownloadSettings.tsx';
import { GeneralSettings } from './GeneralSettings.tsx';
import { LayoutSettings } from './LayoutSettings.tsx';
import { LibrarySettings } from './LibrarySettings.tsx';
import { LyricsSettings } from './LyricsSettings.tsx';
import { MiniPlayerSettings } from './MiniPlayerSettings.tsx';
import { Group } from './Row.tsx';
import { SECTIONS, SECTION_IDS, matchingSections, type SettingsSection } from './sections.ts';
import styles from './SettingsPanel.module.css';
import { ShortcutSettings } from './ShortcutSettings.tsx';
import { SoundSettings } from './SoundSettings.tsx';

function page(section: SettingsSection, onClose: () => void): ReactNode {
  switch (section) {
    case 'general':
      return <GeneralSettings />;
    case 'appearance':
      return <AppearanceSettings />;
    case 'sound':
      return <SoundSettings />;
    case 'library':
      return (
        <Group>
          <LibrarySettings />
        </Group>
      );
    case 'links':
      return (
        <Group>
          <DownloadSettings />
        </Group>
      );
    case 'lyrics':
      return (
        <Group>
          <LyricsSettings />
        </Group>
      );
    case 'mini':
      return (
        <Group>
          <MiniPlayerSettings />
        </Group>
      );
    case 'layout':
      return <LayoutSettings onClose={onClose} />;
    case 'shortcuts':
      return (
        <Group>
          <ShortcutSettings />
        </Group>
      );
    case 'about':
      return <AboutSettings />;
  }
}

/**
 * Settings: a list of pages on the left (with a search over what is on them), the chosen
 * page on the right. It slides over the window like a sheet; the page shown is kept while
 * the window is open, and other parts of the app can open a given page (`showSettings`).
 */
export function SettingsPanel({ onClose }: { onClose(): void }) {
  const { t } = useTranslation();
  const chosen = useUi((s) => s.settingsSection);
  const showSettings = useUi((s) => s.showSettings);
  const [query, setQuery] = useState('');

  const texts = useMemo(
    () =>
      Object.fromEntries(
        SECTION_IDS.map((id) => [
          id,
          [SECTIONS[id].title, ...SECTIONS[id].keywords].map((key) => t(key)),
        ]),
      ) as Record<SettingsSection, string[]>,
    [t],
  );
  const listed = matchingSections(query, texts);
  // While searching, the page shown is one that matches.
  const section = listed.includes(chosen) ? chosen : (listed[0] ?? chosen);
  const info = SECTIONS[section];

  return (
    // Only transform is animated: opacity on an ancestor would cut the glass off from the
    // backdrop it blurs (it becomes a "backdrop root").
    <motion.div
      className={styles.wrap}
      initial={{ x: '110%' }}
      animate={{ x: 0 }}
      exit={{ x: '110%' }}
      transition={{ type: 'spring', stiffness: 380, damping: 38 }}
    >
      <Glass
        as="aside"
        className={styles.panel}
        data-part="settings"
        aria-label={t('settings.title')}
        priority={4}
      >
        <nav className={styles.nav} aria-label={t('settings.title')}>
          <h2 className={styles.title}>{t('settings.title')}</h2>
          <label className={styles.search}>
            <Search size={14} aria-hidden="true" />
            <input
              type="search"
              placeholder={t('settings.search')}
              aria-label={t('settings.search')}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Escape' && query) {
                  // Clears the search rather than closing Settings.
                  event.preventDefault();
                  setQuery('');
                }
              }}
            />
          </label>
          <ul className={styles.navList}>
            {listed.map((id) => {
              const Icon = SECTIONS[id].icon;
              return (
                <li key={id}>
                  <button
                    type="button"
                    className={styles.navItem}
                    aria-current={id === section ? 'page' : undefined}
                    title={t(SECTIONS[id].title)}
                    onClick={() => showSettings(true, id)}
                  >
                    <Icon size={16} aria-hidden="true" />
                    <span className={styles.navLabel}>{t(SECTIONS[id].title)}</span>
                  </button>
                </li>
              );
            })}
          </ul>
          {listed.length === 0 && <p className={styles.noMatch}>{t('settings.noMatch')}</p>}
        </nav>

        <div className={styles.page}>
          <header className={styles.header}>
            <h3 className={styles.pageTitle}>{t(info.title)}</h3>
            <IconButton label={t('settings.close')} onClick={onClose}>
              <X size={18} />
            </IconButton>
          </header>
          <div key={section} className={styles.content} data-scroll-container>
            {listed.length > 0 && page(section, onClose)}
          </div>
        </div>
      </Glass>
    </motion.div>
  );
}
