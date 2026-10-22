import { FolderOpen, LoaderCircle, RefreshCw, RotateCcw } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl, type SegmentedOption } from '../../components/ui/SegmentedControl.tsx';
import {
  downloads,
  MP3_BITRATES,
  ytdlp,
  type AudioFormat,
  type YtDlpInfo,
} from '../../lib/links.ts';
import { pickFolder } from '../../lib/openFiles.ts';
import type { DownloadSettings as Values } from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import { report } from '../library/actions.ts';
import styles from './SettingsPanel.module.css';

type Update =
  | { state: 'idle' }
  | { state: 'running' }
  | { state: 'done'; version: string }
  | { state: 'failed'; message: string };

/** yt-dlp's version and the Update button: sites change, and yt-dlp follows quickly. */
function YtDlpUpdate() {
  const { t } = useTranslation();
  const [info, setInfo] = useState<YtDlpInfo | null>(null);
  const [update, setUpdate] = useState<Update>({ state: 'idle' });

  useEffect(() => {
    ytdlp.info().then(setInfo).catch(report('Reading the yt-dlp version'));
  }, []);

  const run = () => {
    setUpdate({ state: 'running' });
    ytdlp.update().then(
      (version) => {
        setUpdate({ state: 'done', version });
        ytdlp.info().then(setInfo).catch(report('Reading the yt-dlp version'));
      },
      (err: unknown) => setUpdate({ state: 'failed', message: String(err) }),
    );
  };

  const status =
    update.state === 'running'
      ? t('settings.downloads.updating')
      : update.state === 'done'
        ? t('settings.downloads.updated', { version: update.version })
        : update.state === 'failed'
          ? t('settings.downloads.updateFailed', { message: update.message })
          : info === null
            ? null
            : info.version
              ? t('settings.downloads.ytDlpVersion', { version: info.version })
              : t('settings.downloads.ytDlpMissing');

  return (
    <div className={styles.inlineRow}>
      <div className={styles.rowText}>
        <span>yt-dlp</span>
        {status && (
          <span
            className={update.state === 'failed' ? styles.missing : styles.hint}
            role={update.state === 'failed' ? 'alert' : undefined}
          >
            {status}
          </span>
        )}
        {info && !info.ffmpeg && (
          <span className={styles.missing}>{t('settings.downloads.ffmpegMissing')}</span>
        )}
      </div>
      <button
        type="button"
        className={styles.addFolder}
        disabled={update.state === 'running' || info?.path === null}
        onClick={run}
      >
        {update.state === 'running' ? (
          <LoaderCircle size={15} className={styles.spin} aria-hidden="true" />
        ) : (
          <RefreshCw size={15} aria-hidden="true" />
        )}
        {t('settings.downloads.update')}
      </button>
    </div>
  );
}

/** Where downloads go, in what format and under which file name. */
export function DownloadSettings() {
  const { t } = useTranslation();
  const values = useSettings((s) => s.settings.downloads);
  const update = useSettings((s) => s.update);
  const [folder, setFolder] = useState<string | null>(null);
  const [template, setTemplate] = useState(values.template);

  const set = (change: Partial<Values>) =>
    void update((current) => ({ ...current, downloads: { ...current.downloads, ...change } }));

  useEffect(() => {
    downloads.folder().then(setFolder).catch(report('Reading the downloads folder'));
  }, [values.folder]);

  // Follow changes saved elsewhere (or normalized by the backend).
  const [savedTemplate, setSavedTemplate] = useState(values.template);
  if (values.template !== savedTemplate) {
    setSavedTemplate(values.template);
    setTemplate(values.template);
  }

  const chooseFolder = async () => {
    const chosen = await pickFolder();
    if (chosen) {
      set({ folder: chosen });
    }
  };

  const formats: SegmentedOption<AudioFormat>[] = [
    { value: 'original', label: t('settings.downloads.formats.original') },
    { value: 'mp3', label: 'MP3' },
    { value: 'flac', label: 'FLAC' },
  ];
  const bitrates: SegmentedOption<string>[] = MP3_BITRATES.map((bitrate) => ({
    value: String(bitrate),
    label: String(bitrate),
  }));

  return (
    <div className={styles.libraryFolders}>
      <div className={styles.rowText}>
        <span>{t('settings.downloads.folder')}</span>
        <span className={`${styles.hint} ${styles.path}`} title={folder ?? undefined}>
          {folder}
        </span>
      </div>
      <div className={styles.buttonRow}>
        <button type="button" className={styles.addFolder} onClick={() => void chooseFolder()}>
          <FolderOpen size={15} aria-hidden="true" />
          {t('settings.downloads.chooseFolder')}
        </button>
        {values.folder !== '' && (
          <button type="button" className={styles.addFolder} onClick={() => set({ folder: '' })}>
            <RotateCcw size={15} aria-hidden="true" />
            {t('settings.downloads.defaultFolder')}
          </button>
        )}
      </div>

      <div className={styles.row}>
        <div className={styles.rowText}>
          <span>{t('settings.downloads.format')}</span>
          <span className={styles.hint}>
            {t(`settings.downloads.formatHints.${values.format}`)}
          </span>
        </div>
        <SegmentedControl
          label={t('settings.downloads.format')}
          value={values.format}
          options={formats}
          onChange={(format) => set({ format })}
        />
      </div>
      {values.format === 'mp3' && (
        <div className={styles.row}>
          <span>{t('settings.downloads.bitrate')}</span>
          <SegmentedControl
            label={t('settings.downloads.bitrate')}
            value={String(values.mp3Bitrate)}
            options={bitrates}
            onChange={(bitrate) => set({ mp3Bitrate: Number(bitrate) })}
          />
        </div>
      )}

      <label className={styles.row}>
        <span className={styles.rowText}>
          <span>{t('settings.downloads.template')}</span>
          <span className={styles.hint}>{t('settings.downloads.templateHint')}</span>
        </span>
        <input
          className={styles.textInput}
          value={template}
          spellCheck={false}
          onChange={(event) => setTemplate(event.target.value)}
          onBlur={() => template !== values.template && set({ template })}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.currentTarget.blur();
            }
          }}
        />
      </label>

      <YtDlpUpdate />
    </div>
  );
}
