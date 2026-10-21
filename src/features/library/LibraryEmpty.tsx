import { FolderOpen, FolderPlus, LoaderCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { library } from '../../lib/library.ts';
import { pickAndPlay } from '../../lib/openFiles.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useScanStatus } from '../../stores/library.ts';
import { addMusicFolder, report } from './actions.ts';
import { EmptyState } from './common.tsx';
import styles from './Library.module.css';

const FOLDER_CHANGES = ['folders'] as const;

/** What an empty library view shows: a scan in progress, or how to add music. */
export function LibraryEmpty() {
  const { t } = useTranslation();
  const folders = useLibraryData('folders', library.folders, FOLDER_CHANGES).data;
  const scan = useScanStatus((s) => s.status);

  if (scan.scanning || scan.queued > 0) {
    return (
      <EmptyState
        icon={<LoaderCircle size={36} className={styles.spin} aria-hidden="true" />}
        title={t('library.scanning')}
        body={
          scan.total > 0 ? t('library.scanProgress', { done: scan.done, total: scan.total }) : ''
        }
      />
    );
  }
  if (folders?.length === 0) {
    return (
      <EmptyState
        icon={<FolderPlus size={40} aria-hidden="true" />}
        title={t('library.emptyTitle')}
        body={t('library.emptyBody')}
      >
        <div className={styles.actions}>
          <button
            type="button"
            className={styles.primaryButton}
            onClick={() => void addMusicFolder()}
          >
            <FolderPlus size={15} aria-hidden="true" />
            {t('library.addFolder')}
          </button>
          <button
            type="button"
            className={styles.secondaryButton}
            onClick={() => pickAndPlay().catch(report('Opening files'))}
          >
            <FolderOpen size={15} aria-hidden="true" />
            {t('player.open')}
          </button>
        </div>
      </EmptyState>
    );
  }
  return <EmptyState title={t('library.nothingHere')} />;
}
