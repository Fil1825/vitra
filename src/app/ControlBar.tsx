import { Play, SkipBack, SkipForward, Volume2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { IconButton } from '../components/ui/IconButton.tsx';
import styles from './ControlBar.module.css';

/** Bottom transport bar. Disabled until the playback engine lands in stage 2. */
export function ControlBar({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <Glass
      as="footer"
      className={`${styles.bar} ${className ?? ''}`}
      aria-label={t('player.controls')}
      priority={3}
    >
      <div className={styles.track}>
        <div className={styles.artwork} aria-hidden="true" />
        <span className={styles.trackTitle}>{t('player.notPlaying')}</span>
      </div>

      <div className={styles.transport}>
        <div className={styles.buttons}>
          <IconButton label={t('player.previous')} disabled>
            <SkipBack size={18} fill="currentColor" />
          </IconButton>
          <IconButton label={t('player.play')} size="large" disabled>
            <Play size={22} fill="currentColor" />
          </IconButton>
          <IconButton label={t('player.next')} disabled>
            <SkipForward size={18} fill="currentColor" />
          </IconButton>
        </div>
        <div
          className={styles.progress}
          role="progressbar"
          aria-label={t('player.progress')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={0}
        />
      </div>

      <div className={styles.volume}>
        <Volume2 size={16} aria-hidden="true" />
        <div className={styles.volumeTrack} aria-label={t('player.volume')} role="presentation" />
      </div>
    </Glass>
  );
}
