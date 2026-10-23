import { FolderPlus, RefreshCw, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { Switch } from '../../components/ui/Switch.tsx';
import { confirm } from '../../lib/confirm.ts';
import { library, type ScanStatus } from '../../lib/library.ts';
import type { ArtistSplitting } from '../../lib/settings.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useScanStatus } from '../../stores/library.ts';
import { useSettings } from '../../stores/settings.ts';
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

/** Whether "A feat. B" is two artists, and the names that are one despite a comma. */
function ArtistSplittingSettings() {
  const { t } = useTranslation();
  const splitting = useSettings((s) => s.settings.library.artistSplitting);
  const update = useSettings((s) => s.update);
  const stored = splitting.exceptions.join('\n');
  const [text, setText] = useState(stored);
  // Follow changes saved elsewhere (or cleaned up by the backend).
  const [saved, setSaved] = useState(stored);
  if (stored !== saved) {
    setSaved(stored);
    setText(stored);
  }
  const save = (change: Partial<ArtistSplitting>) =>
    void update((current) => ({
      ...current,
      library: {
        ...current.library,
        artistSplitting: { ...current.library.artistSplitting, ...change },
      },
    }));
  const exceptions = text
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean);

  return (
    <>
      <div className={styles.inlineRow}>
        <div className={styles.rowText}>
          <span>{t('settings.library.splitArtists')}</span>
          <span className={styles.hint}>{t('settings.library.splitArtistsHint')}</span>
        </div>
        <Switch
          label={t('settings.library.splitArtists')}
          checked={splitting.enabled}
          onChange={(enabled) => save({ enabled })}
        />
      </div>
      {splitting.enabled && (
        <label className={styles.row}>
          <span className={styles.rowText}>
            <span>{t('settings.library.exceptions')}</span>
            <span className={styles.hint}>{t('settings.library.exceptionsHint')}</span>
          </span>
          <textarea
            className={`${styles.textInput} ${styles.textArea}`}
            value={text}
            rows={3}
            spellCheck={false}
            onChange={(event) => setText(event.target.value)}
            onBlur={() => exceptions.join('\n') !== stored && save({ exceptions })}
          />
        </label>
      )}
    </>
  );
}

/** Music folders: add, remove, rescan, and scan progress. */
export function LibrarySettings() {
  const { t } = useTranslation();
  const folders = useLibraryData('folders', library.folders, FOLDER_CHANGES).data ?? [];
  const status = useScanStatus((s) => s.status);
  const scanText = useScanText()(status);
  const addOpenedFiles = useSettings((s) => s.settings.library.addOpenedFiles);
  const update = useSettings((s) => s.update);

  const remove = async (id: number, path: string) => {
    const confirmed = await confirm(t('settings.library.removeFolderConfirm', { path }), 'remove');
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
      <ArtistSplittingSettings />
      <div className={styles.inlineRow}>
        <div className={styles.rowText}>
          <span>{t('settings.library.addOpenedFiles')}</span>
          <span className={styles.hint}>{t('settings.library.addOpenedFilesHint')}</span>
        </div>
        <Switch
          label={t('settings.library.addOpenedFiles')}
          checked={addOpenedFiles}
          onChange={(checked) =>
            void update((current) => ({
              ...current,
              library: { ...current.library, addOpenedFiles: checked },
            }))
          }
        />
      </div>
    </div>
  );
}
