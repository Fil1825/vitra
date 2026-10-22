import { Download, Play, RotateCcw, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { downloads, isActive, type DownloadJob } from '../../lib/links.ts';
import { useDownloads } from '../../stores/downloads.ts';
import { useSettings } from '../../stores/settings.ts';
import { playTracks, report } from '../library/actions.ts';
import { EmptyState, PageHeader } from '../library/common.tsx';
import libraryStyles from '../library/Library.module.css';
import styles from './DownloadsView.module.css';

function useStatusText(): (job: DownloadJob) => string {
  const { t } = useTranslation();
  return ({ status }) => {
    switch (status.state) {
      case 'queued':
        return t('downloads.queued');
      case 'downloading':
        return status.progress === null
          ? t('downloads.downloading')
          : t('downloads.progress', { percent: Math.floor(status.progress * 100) });
      case 'processing':
        return t('downloads.processing');
      case 'done':
        return t('downloads.done');
      case 'failed':
        return t('downloads.failed', { message: status.error });
      case 'cancelled':
        return t('downloads.cancelled');
    }
  };
}

function JobRow({ job }: { job: DownloadJob }) {
  const { t } = useTranslation();
  const statusText = useStatusText();
  const { status } = job;
  const progress =
    status.state === 'downloading' ? status.progress : status.state === 'processing' ? 1 : null;
  return (
    <li className={styles.job} data-state={status.state}>
      <div className={styles.text}>
        <span className={styles.title}>{job.title}</span>
        <span className={styles.artist}>{job.artist}</span>
        <span
          className={styles.status}
          title={status.state === 'done' ? status.path : undefined}
          role={status.state === 'failed' ? 'alert' : undefined}
        >
          {statusText(job)}
        </span>
        {isActive(status) && (
          <div
            className={styles.bar}
            role="progressbar"
            aria-label={job.title}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={progress === null ? undefined : Math.round(progress * 100)}
          >
            <div
              className={progress === null ? styles.indeterminate : styles.fill}
              style={progress === null ? undefined : { transform: `scaleX(${progress})` }}
            />
          </div>
        )}
      </div>
      <div className={styles.buttons}>
        {status.state === 'done' && (
          <IconButton label={t('library.play')} onClick={() => playTracks([job.trackId])}>
            <Play size={15} />
          </IconButton>
        )}
        {(status.state === 'failed' || status.state === 'cancelled') && (
          <IconButton
            label={t('downloads.retry')}
            onClick={() => downloads.retry(job.id).catch(report('Retrying a download'))}
          >
            <RotateCcw size={15} />
          </IconButton>
        )}
        {isActive(status) && (
          <IconButton
            label={t('downloads.cancel')}
            onClick={() => downloads.cancel(job.id).catch(report('Cancelling a download'))}
          >
            <X size={15} />
          </IconButton>
        )}
      </div>
    </li>
  );
}

/** The download queue: progress, cancel, retry. */
export function DownloadsView() {
  const { t } = useTranslation();
  const jobs = useDownloads((s) => s.jobs);
  const folderSetting = useSettings((s) => s.settings.downloads.folder);
  const [folder, setFolder] = useState<string | null>(null);
  const finished = jobs.some((job) => !isActive(job.status));

  useEffect(() => {
    downloads.folder().then(setFolder).catch(report('Reading the downloads folder'));
  }, [folderSetting]);

  return (
    <div className={libraryStyles.view}>
      <PageHeader title={t('downloads.title')}>
        {finished && (
          <button
            type="button"
            className={libraryStyles.secondaryButton}
            onClick={() => downloads.clear().catch(report('Clearing downloads'))}
          >
            {t('downloads.clear')}
          </button>
        )}
      </PageHeader>
      {folder && <p className={styles.folder}>{t('downloads.savingTo', { folder })}</p>}
      {jobs.length === 0 ? (
        <EmptyState
          icon={<Download size={36} aria-hidden="true" />}
          title={t('downloads.empty')}
          body={t('downloads.emptyBody')}
        />
      ) : (
        <ul className={styles.list}>
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} />
          ))}
        </ul>
      )}
    </div>
  );
}
