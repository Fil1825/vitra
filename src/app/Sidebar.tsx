import { Clock3, Disc3, ListMusic, Music2, Users } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import styles from './Sidebar.module.css';

export function Sidebar({ className }: { className?: string }) {
  const { t } = useTranslation();
  const items = [
    { key: 'albums', label: t('nav.albums'), icon: Disc3 },
    { key: 'artists', label: t('nav.artists'), icon: Users },
    { key: 'songs', label: t('nav.songs'), icon: Music2 },
    { key: 'recent', label: t('nav.recent'), icon: Clock3 },
    { key: 'playlists', label: t('nav.playlists'), icon: ListMusic },
  ];

  return (
    <Glass
      as="nav"
      className={`${styles.sidebar} ${className ?? ''}`}
      aria-label={t('nav.label')}
      priority={2}
    >
      {/* Room for the macOS traffic lights; also lets the user drag the window from here. */}
      <div className={styles.dragArea} data-tauri-drag-region />
      <p className={styles.section}>{t('nav.library')}</p>
      <ul className={styles.list}>
        {items.map(({ key, label, icon: Icon }) => (
          <li key={key}>
            <button type="button" className={styles.item} disabled title={t('nav.comingSoon')}>
              <Icon size={16} className={styles.icon} />
              {label}
            </button>
          </li>
        ))}
      </ul>
    </Glass>
  );
}
