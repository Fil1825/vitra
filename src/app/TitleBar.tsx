import { FolderOpen, Settings2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../components/ui/IconButton.tsx';
import { pickAndPlay } from '../lib/openFiles.ts';
import styles from './TitleBar.module.css';

export interface TitleBarProps {
  className?: string;
  onOpenSettings(): void;
}

/** Top strip of the content area; doubles as the window drag region. */
export function TitleBar({ className, onOpenSettings }: TitleBarProps) {
  const { t } = useTranslation();
  return (
    <header className={`${styles.titleBar} ${className ?? ''}`} data-tauri-drag-region>
      <h1 className={styles.title} data-tauri-drag-region>
        {t('nowPlaying.label')}
      </h1>
      <div className={styles.actions}>
        <IconButton
          label={t('player.open')}
          onClick={() =>
            pickAndPlay().catch((err: unknown) => console.error('Opening files failed', err))
          }
        >
          <FolderOpen size={18} />
        </IconButton>
        <IconButton label={t('settings.open')} onClick={onOpenSettings}>
          <Settings2 size={18} />
        </IconButton>
      </div>
    </header>
  );
}
