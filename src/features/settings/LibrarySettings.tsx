import { ask } from '@tauri-apps/plugin-dialog';
import { FolderPlus, RefreshCw, Trash2 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { library, type ScanStatus } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useScanStatus } from '../../stores/library.ts';
import { addMusicFolder, report } from '../library/actions.ts';
import styles from './SettingsPanel.module.css';

const FOLDER_CHANGES = ['folders'] as const;

function useScanText(): (status: ScanStatus) => string | null {
  const { t } = useTranslation();
  return (status) => {
    if (!status.scanning) {
      return null;
    }
    switch (status.phase) {
      case 'reading':
        return t('settings.library.scanning', { done: status.done, total: status.total });
      case 'covers':
        return t('settings.library.scanCovers', { done: status.done, total: status.total });
      default:
        return t('settings.library.scanListing');
    }
  };
}

/** Music folders: add, remove, rescan, and scan progress. */
export function LibrarySettings() {
  const { t } = useTranslation();
  const folders = useLibraryData('folders', library.folders, FOLDER_CHANGES).data ?? [];
  const status = useScanStatus((s) => s.status);
  const scanText = useScanText()(status);

  const remove = async (id: number, path: string) => {
    const confirmed = await ask(t('settings.library.removeFolderConfirm', { path }), {
      kind: 'warning',
    });
    if (confirmed) {
      await library.removeFolder(id);
    }
  };

  return (
    <div className={styles.libraryFolders}>
      <div className={styles.rowText}>
        <span>{t('settings.library.folders')}</span>
        <span className={styles.hint}>{t('settings.library.watchHint')}</span>
      </div>
      {folders.length === 0 ? (
        <p className={styles.hint}>{t('settings.library.noFolders')}</p>
      ) : (
        <ul className={styles.folderList}>
          {folders.map((folder) => (
            <li key={folder.id} className={styles.folder}>
              <div className={styles.folderText}>
                <span className={styles.folderPath} title={folder.path}>
                  {folder.path}
                </span>
                <span className={styles.hint}>
                  {status.folderId === folder.id && scanText
                    ? scanText
                    : t('library.songCount', { count: folder.trackCount })}
                </span>
              </div>
              <IconButton
                label={t('settings.library.rescan')}
                onClick={() => library.rescan(folder.id).catch(report('Rescanning'))}
              >
                <RefreshCw size={15} />
              </IconButton>
              <IconButton
                label={t('settings.library.removeFolder')}
                onClick={() => remove(folder.id, folder.path).catch(report('Removing a folder'))}
              >
                <Trash2 size={15} />
              </IconButton>
            </li>
          ))}
        </ul>
      )}
      <button type="button" className={styles.addFolder} onClick={() => void addMusicFolder()}>
        <FolderPlus size={15} aria-hidden="true" />
        {t('settings.library.addFolder')}
      </button>
    </div>
  );
}
