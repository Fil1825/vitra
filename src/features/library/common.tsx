import { Play, Shuffle } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { playTracks, shuffleTracks } from './actions.ts';
import styles from './Library.module.css';

/** Play and Shuffle for a whole list. */
/** Play and Shuffle, followed by `children` (more buttons of the page). */
export function PlayButtons({ ids, children }: { ids: number[]; children?: ReactNode }) {
  const { t } = useTranslation();
  return (
    <div className={styles.actions}>
      <button
        type="button"
        className={styles.primaryButton}
        disabled={ids.length === 0}
        onClick={() => playTracks(ids)}
      >
        <Play size={15} fill="currentColor" aria-hidden="true" />
        {t('library.play')}
      </button>
      <button
        type="button"
        className={styles.secondaryButton}
        disabled={ids.length === 0}
        onClick={() => shuffleTracks(ids)}
      >
        <Shuffle size={15} aria-hidden="true" />
        {t('library.shuffle')}
      </button>
      {children}
    </div>
  );
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <header className={styles.pageHeader}>
      <h2 className={styles.pageTitle}>{title}</h2>
      {children}
    </header>
  );
}

export function EmptyState({
  icon,
  title,
  body,
  children,
}: {
  icon?: ReactNode;
  title: string;
  body?: string;
  children?: ReactNode;
}) {
  return (
    <div className={styles.empty}>
      {icon}
      <h3 className={styles.emptyTitle}>{title}</h3>
      {body && <p className={styles.emptyBody}>{body}</p>}
      {children}
    </div>
  );
}

export function LoadError({ error }: { error: unknown }) {
  const { t } = useTranslation();
  return (
    <p className={styles.error} role="alert">
      {t('library.loadFailed', { message: String(error) })}
    </p>
  );
}
