import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import styles from './LyricsPanel.module.css';
import { LyricsView } from './LyricsView.tsx';

/** The lyrics next to the main area, in place of the queue. */
export function LyricsPanel({ className }: { className?: string }) {
  const { t } = useTranslation();
  return (
    <Glass as="aside" className={`${styles.panel} ${className ?? ''}`} priority={2}>
      <LyricsView
        variant="panel"
        className={styles.view}
        heading={<h2 className={styles.title}>{t('lyrics.title')}</h2>}
      />
    </Glass>
  );
}
