import { LoaderCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { importer, type ImportProgress } from '../../lib/import.ts';
import { report } from '../library/actions.ts';
import styles from './ImportStatus.module.css';

/** The line under an imported playlist's title: still looking, not found, doubtful. */
export function ImportStatus({
  progress,
  playlistId,
}: {
  progress: ImportProgress;
  playlistId: number;
}) {
  const { t } = useTranslation();
  if (progress.total === 0) {
    return null;
  }
  return (
    <p className={styles.status} aria-live="polite">
      {progress.pending > 0 ? (
        <>
          <LoaderCircle size={14} className={styles.spin} aria-hidden="true" />
          {t('import.progress.looking', {
            count: progress.pending,
            total: progress.total,
          })}
        </>
      ) : (
        t('import.progress.found', { count: progress.found, total: progress.total })
      )}
      {progress.doubtful > 0 && (
        <span className={styles.doubtful}>
          {t('import.progress.doubtful', { count: progress.doubtful })}
        </span>
      )}
      {progress.missing > 0 && progress.pending === 0 && (
        <button
          type="button"
          className={styles.retry}
          onClick={() => importer.retry(playlistId).catch(report('Looking again'))}
        >
          {t('import.lookAgain')}
        </button>
      )}
    </p>
  );
}
