import { LoaderCircle, Pause, Play, Settings2, Square } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { queue } from '../../lib/queue.ts';
import { player } from '../../lib/player.ts';
import { sameMode, WAVE_MOODS, type AnalysisStatus, type WaveMode } from '../../lib/wave.ts';
import { usePlayer } from '../../stores/player.ts';
import { useQueue } from '../../stores/queue.ts';
import { useUi } from '../../stores/ui.ts';
import { report } from '../library/actions.ts';
import { Cover } from '../library/Cover.tsx';
import libraryStyles from '../library/Library.module.css';
import { startWave, stopWave, useWaveUi } from './actions.ts';
import { useWaveAnalysis } from './useWaveAnalysis.ts';
import styles from './WavePage.module.css';

/** How many of the wave's next tracks the page lists. */
const SHOWN_AHEAD = 6;

/** Whether the analysis still has files to go (the wave gets better as it finishes). */
function analysisRunning(status: AnalysisStatus): boolean {
  return status.filesTotal > 0 && status.filesDone < status.filesTotal;
}

/** My Wave: the moods to start from, what the wave plays next, how far the analysis is. */
export function WavePage() {
  const { t } = useTranslation();
  const mode = useQueue((s) => s.queue.wave);
  const { starting, notice } = useWaveUi();
  const status = usePlayer((s) => s.state.status);
  const track = usePlayer((s) => s.state.track);
  const analysis = useWaveAnalysis();
  const active = mode !== null;
  const playing = active && (status === 'playing' || status === 'loading');

  const likeTrack: WaveMode | null =
    mode?.kind === 'likeTrack'
      ? mode
      : track?.libraryId != null
        ? { kind: 'likeTrack', track: track.libraryId }
        : null;
  const modes: { mode: WaveMode; label: string }[] = [
    ...WAVE_MOODS.map((kind) => ({ mode: { kind }, label: t(`wave.moods.${kind}`) })),
    ...(likeTrack ? [{ mode: likeTrack, label: t('wave.moods.likeTrack') }] : []),
  ];

  const main = () => {
    if (!active) {
      void startWave({ kind: 'taste' });
    } else {
      player.toggle().catch(report('Play/pause'));
    }
  };

  return (
    <div className={libraryStyles.view}>
      <section className={styles.hero} aria-labelledby="wave-title">
        <div className={styles.orb} data-playing={playing || undefined} aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <div className={styles.heroText}>
          <h2 id="wave-title" className={libraryStyles.pageTitle}>
            {t('wave.title')}
          </h2>
          <p className={styles.lead}>{t('wave.page.lead')}</p>
          <div className={libraryStyles.actions}>
            <button
              type="button"
              className={libraryStyles.primaryButton}
              disabled={starting !== null}
              onClick={main}
            >
              {starting ? (
                <LoaderCircle size={15} className={styles.spin} aria-hidden="true" />
              ) : playing ? (
                <Pause size={15} fill="currentColor" aria-hidden="true" data-morph="" />
              ) : (
                <Play size={15} fill="currentColor" aria-hidden="true" data-morph="" />
              )}
              {playing ? t('player.pause') : active ? t('wave.page.resume') : t('wave.page.start')}
            </button>
            {active && (
              <button type="button" className={libraryStyles.secondaryButton} onClick={stopWave}>
                <Square size={13} fill="currentColor" aria-hidden="true" />
                {t('wave.page.stop')}
              </button>
            )}
          </div>
        </div>
      </section>

      <section className={styles.moods} aria-label={t('wave.page.moods')}>
        {modes.map((option) => {
          const chosen = sameMode(mode, option.mode);
          const busy = starting !== null && sameMode(starting, option.mode);
          return (
            <button
              key={option.mode.kind}
              type="button"
              className={styles.mood}
              aria-pressed={chosen}
              aria-busy={busy || undefined}
              disabled={starting !== null}
              title={
                option.mode.kind === 'likeTrack' && track && !chosen
                  ? t('wave.page.likeTrackHint', { title: track.title })
                  : undefined
              }
              onClick={() => void startWave(option.mode)}
            >
              {busy && <LoaderCircle size={13} className={styles.spin} aria-hidden="true" />}
              {option.label}
            </button>
          );
        })}
      </section>

      {notice && <Notice notice={notice} analysis={analysis} />}
      {!notice && analysisRunning(analysis) && (
        <p className={styles.status}>
          {t('wave.page.analyzing', { done: analysis.filesDone, total: analysis.filesTotal })}
        </p>
      )}

      {active && <UpNext />}
    </div>
  );
}

/** Why the wave didn't start, and where to look. */
function Notice({
  notice,
  analysis,
}: {
  notice: 'nothingMeasured' | 'trackUnmeasured';
  analysis: AnalysisStatus;
}) {
  const { t } = useTranslation();
  const showSettings = useUi((s) => s.showSettings);
  const why =
    notice === 'trackUnmeasured'
      ? t('wave.page.trackUnmeasured')
      : analysis.state === 'paused'
        ? t('wave.page.nothingPaused')
        : analysis.state === 'noFfmpeg'
          ? t('wave.analysis.noFfmpeg')
          : analysis.filesTotal + analysis.linksTotal === 0
            ? t('wave.page.nothingInLibrary')
            : t('wave.page.nothingYet', {
                done: analysis.filesDone,
                total: analysis.filesTotal,
              });
  return (
    <div className={styles.notice} role="status">
      <p>{why}</p>
      <button
        type="button"
        className={libraryStyles.secondaryButton}
        onClick={() => showSettings(true, 'wave')}
      >
        <Settings2 size={14} aria-hidden="true" />
        {t('wave.page.settings')}
      </button>
    </div>
  );
}

/** The wave's next picks (it adds more as they play). */
function UpNext() {
  const { t } = useTranslation();
  const state = useQueue((s) => s.queue);
  const offset = state.current === null ? 0 : state.current + 1;
  const upcoming = state.items.slice(offset, offset + SHOWN_AHEAD);
  return (
    <section className={libraryStyles.section}>
      <h3 className={libraryStyles.sectionTitle}>{t('wave.page.upNext')}</h3>
      {upcoming.length === 0 ? (
        <p className={styles.status}>{t('wave.page.picking')}</p>
      ) : (
        <ol className={styles.list}>
          {upcoming.map((item) => (
            <li key={item.uid}>
              <button
                type="button"
                className={styles.row}
                onClick={() => queue.jump(item.uid).catch(report('Jumping in the queue'))}
              >
                <Cover id={item.cover} className={styles.cover} />
                <span className={styles.rowText}>
                  <span className={styles.rowTitle}>{item.title}</span>
                  <span className={styles.rowArtist}>{item.artist ?? ''}</span>
                </span>
              </button>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
