import { CirclePause, LoaderCircle, TriangleAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { importer, type ImportProgress, type MatcherStatus } from '../../lib/import.ts';
import { BROWSER_NAMES, COOKIE_BROWSERS } from '../../lib/settings.ts';
import { useSettings } from '../../stores/settings.ts';
import { useUi } from '../../stores/ui.ts';
import { report } from '../library/actions.ts';
import styles from './ImportStatus.module.css';
import { useMatcherStatus, useSecondsUntil } from './useMatcherStatus.ts';

function browserName(id: string): string {
  return (COOKIE_BROWSERS as readonly string[]).includes(id)
    ? BROWSER_NAMES[id as (typeof COOKIE_BROWSERS)[number]]
    : id;
}

const resume = () => importer.resume().catch(report('Resuming the search'));
const openSettings = () => useUi.getState().showSettings(true, 'links');

/** Why the search stopped, and what to do about it. */
function Blocked({ status }: { status: Extract<MatcherStatus, { state: 'blocked' }> }) {
  const { t } = useTranslation();
  const browser = useSettings((s) => s.settings.downloads.cookiesFromBrowser);
  const message =
    status.reason === 'cookies'
      ? t(browser === 'safari' ? 'import.status.cookiesSafari' : 'import.status.cookies', {
          browser: browserName(browser),
        })
      : status.reason === 'ytDlpMissing'
        ? t('import.status.ytDlpMissing')
        : t('import.status.signIn');
  return (
    <span className={styles.problem} role="alert" title={status.detail}>
      <TriangleAlert size={14} className={styles.alert} aria-hidden="true" />
      <span>{message}</span>
      <span className={styles.buttons}>
        <button type="button" className={styles.retry} onClick={openSettings}>
          {t('import.status.openSettings')}
        </button>
        <button type="button" className={styles.retry} onClick={() => void resume()}>
          {t('import.status.retry')}
        </button>
      </span>
    </span>
  );
}

function Paused({ status }: { status: Extract<MatcherStatus, { state: 'paused' }> }) {
  const { t } = useTranslation();
  const seconds = useSecondsUntil(status.retryAt);
  return (
    <span className={styles.problem}>
      <CirclePause size={14} aria-hidden="true" />
      <span>
        {t(
          status.reason === 'rateLimited' ? 'import.status.rateLimited' : 'import.status.offline',
          { seconds },
        )}
      </span>
      <button type="button" className={styles.retry} onClick={() => void resume()}>
        {t('import.status.retryNow')}
      </button>
    </span>
  );
}

/**
 * The lines under an imported playlist's title: how many were found, what the search is
 * doing right now (a track, a pause with its countdown, a stop with what to do), doubtful
 * ones and Look Again.
 */
export function ImportStatus({
  progress,
  playlistId,
}: {
  progress: ImportProgress;
  playlistId: number;
}) {
  const { t } = useTranslation();
  const status = useMatcherStatus();
  if (progress.total === 0) {
    return null;
  }
  const waiting = progress.pending > 0;
  return (
    <div className={styles.block} aria-live="polite">
      <p className={styles.status}>
        {waiting && status.state !== 'paused' && status.state !== 'blocked' && (
          <LoaderCircle size={14} className={styles.spin} aria-hidden="true" />
        )}
        <span>{t('import.progress.found', { count: progress.found, total: progress.total })}</span>
        {waiting && status.state === 'searching' && (
          <span className={styles.now}>
            {status.playlistId === playlistId
              ? t('import.status.searching', { title: status.title })
              : t('import.status.queued')}
          </span>
        )}
        {waiting && status.state === 'idle' && (
          <span className={styles.now}>
            {t('import.progress.looking', { count: progress.pending, total: progress.total })}
          </span>
        )}
        {progress.doubtful > 0 && (
          <span className={styles.doubtful}>
            {t('import.progress.doubtful', { count: progress.doubtful })}
          </span>
        )}
        {progress.missing > 0 && !waiting && (
          <button
            type="button"
            className={styles.retry}
            onClick={() => importer.retry(playlistId).catch(report('Looking again'))}
          >
            {t('import.lookAgain')}
          </button>
        )}
      </p>
      {waiting && status.state === 'paused' && <Paused status={status} />}
      {waiting && status.state === 'blocked' && <Blocked status={status} />}
    </div>
  );
}
