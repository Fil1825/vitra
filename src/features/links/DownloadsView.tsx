import { ChevronDown, ChevronRight, Download, Play, RotateCcw, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { downloads, isActive, type DownloadJob } from '../../lib/links.ts';
import { useDownloads } from '../../stores/downloads.ts';
import { useSettings } from '../../stores/settings.ts';
import { playTracks, report } from '../library/actions.ts';
import { EmptyState, PageHeader } from '../library/common.tsx';
import { Cover } from '../library/Cover.tsx';
import { knownLinkError } from './errors.ts';
import { groupDownloads, type DownloadItem } from './groups.ts';
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
        return knownLinkError(status.error, t) ?? t('downloads.failed', { message: status.error });
      case 'cancelled':
        return t('downloads.cancelled');
    }
  };
}

function ProgressBar({ label, progress }: { label: string; progress: number | null }) {
  return (
    <div
      className={styles.bar}
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={progress === null ? undefined : Math.round(progress * 100)}
    >
      <div
        className={progress === null ? styles.indeterminate : styles.fill}
        style={progress === null ? undefined : { transform: `scaleX(${progress})` }}
      />
    </div>
  );
}

/** An album or playlist: its overall progress, and its tracks on demand. */
function GroupRow({ item }: { item: Extract<DownloadItem, { kind: 'group' }> }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const { group, jobs, progress, done, failed, active } = item;
  const finished = !active && failed === 0;
  return (
    <li className={styles.group}>
      <div
        className={styles.job}
        data-state={finished ? 'done' : active ? 'downloading' : 'failed'}
      >
        <Cover id={group.cover} className={styles.cover} />
        <div className={styles.text}>
          <span className={styles.title}>{group.title}</span>
          <span className={styles.artist}>{group.artist}</span>
          <span className={styles.status}>
            {[
              t('downloads.albumProgress', { done, total: jobs.length }),
              failed > 0 && t('downloads.failedCount', { count: failed }),
            ]
              .filter(Boolean)
              .join(' · ')}
          </span>
          {active && <ProgressBar label={group.title} progress={progress} />}
        </div>
        <div className={styles.buttons}>
          {done > 0 && !active && (
            <IconButton
              label={t('library.play')}
              onClick={() =>
                playTracks(
                  jobs.filter((job) => job.status.state === 'done').map((job) => job.trackId),
                )
              }
            >
              <Play size={15} />
            </IconButton>
          )}
          {failed > 0 && (
            <IconButton
              label={t('downloads.retryAll')}
              onClick={() =>
                downloads.retryGroup(group.id).catch(report('Retrying an album download'))
              }
            >
              <RotateCcw size={15} />
            </IconButton>
          )}
          {active && (
            <IconButton
              label={t('downloads.cancelAll')}
              onClick={() =>
                downloads.cancelGroup(group.id).catch(report('Cancelling an album download'))
              }
            >
              <X size={15} />
            </IconButton>
          )}
          <IconButton
            label={t(open ? 'downloads.hideTracks' : 'downloads.showTracks')}
            aria-expanded={open}
            onClick={() => setOpen((value) => !value)}
          >
            {open ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
          </IconButton>
        </div>
      </div>
      {open && (
        <ul className={styles.members}>
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} />
          ))}
        </ul>
      )}
    </li>
  );
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
        {isActive(status) && <ProgressBar label={job.title} progress={progress} />}
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
          {groupDownloads(jobs).map((item) =>
            item.kind === 'job' ? (
              <JobRow key={item.job.id} job={item.job} />
            ) : (
              <GroupRow key={`group-${item.group.id}`} item={item} />
            ),
          )}
        </ul>
      )}
    </div>
  );
}
