import { Pause, Play } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Switch } from '../../components/ui/Switch.tsx';
import type { WaveSettings as Wave } from '../../lib/settings.ts';
import type { AnalysisStatus } from '../../lib/wave.ts';
import { useSettings } from '../../stores/settings.ts';
import { useWaveAnalysis } from '../wave/useWaveAnalysis.ts';
import { Group, Row } from './Row.tsx';
import styles from './SettingsPanel.module.css';

const time = (ms: number, language: string) =>
  new Date(ms).toLocaleTimeString(language, { hour: '2-digit', minute: '2-digit' });

/** What the analysis does right now, in a sentence. */
function useStateText(): (status: AnalysisStatus) => string {
  const { t } = useTranslation();
  return (status) => {
    switch (status.state) {
      case 'analyzing':
        return status.current
          ? t('wave.analysis.analyzing', { title: status.current })
          : t('wave.analysis.analyzingSome');
      case 'paused':
        return t('wave.analysis.paused');
      case 'scanning':
        return t('wave.analysis.scanning');
      case 'powerSaving':
        return t('wave.analysis.powerSaving');
      case 'noFfmpeg':
        return t('wave.analysis.noFfmpeg');
      case 'idle':
        return status.filesDone >= status.filesTotal
          ? t('wave.analysis.done')
          : t('wave.analysis.waiting');
    }
  };
}

/** Settings → My Wave: the background analysis and what it may do online. */
export function WaveSettings() {
  const { t, i18n } = useTranslation();
  const settings = useSettings((s) => s.settings.wave);
  const update = useSettings((s) => s.update);
  const status = useWaveAnalysis();
  const stateText = useStateText();
  const set = (change: Partial<Wave>) =>
    void update((current) => ({ ...current, wave: { ...current.wave, ...change } }));
  const share = status.filesTotal > 0 ? status.filesDone / status.filesTotal : 0;

  const linksText = (() => {
    const at = status.linksRetryAt ? time(status.linksRetryAt, i18n.language) : '';
    switch (status.links) {
      case 'rateLimited':
        return t('wave.links.rateLimited', { time: at });
      case 'offline':
        return t('wave.links.offline', { time: at });
      case 'noYtDlp':
        return t('wave.links.noYtDlp');
      default:
        return null;
    }
  })();

  return (
    <>
      <Group title={t('wave.analysis.title')}>
        <Row label={t('wave.analysis.label')} hint={t('wave.analysis.hint')}>
          <div className={styles.waveProgress}>
            <div
              className={styles.waveBar}
              role="progressbar"
              aria-label={t('wave.analysis.label')}
              aria-valuemin={0}
              aria-valuemax={status.filesTotal}
              aria-valuenow={status.filesDone}
            >
              <span style={{ width: `${Math.round(share * 100)}%` }} />
            </div>
            <span className={styles.waveCount}>
              {t('wave.analysis.files', { done: status.filesDone, total: status.filesTotal })}
            </span>
          </div>
          <span className={styles.hint} aria-live="polite">
            {stateText(status)}
          </span>
          {(status.failed > 0 || status.skipped > 0) && (
            <span className={styles.hint}>
              {[
                status.failed > 0 && t('wave.analysis.failed', { count: status.failed }),
                status.skipped > 0 && t('wave.analysis.skipped', { count: status.skipped }),
              ]
                .filter(Boolean)
                .join(' · ')}
            </span>
          )}
          <div className={styles.buttonRow}>
            <button
              type="button"
              className={styles.textButton}
              onClick={() => set({ analyze: !settings.analyze })}
            >
              {settings.analyze ? (
                <Pause size={14} aria-hidden="true" />
              ) : (
                <Play size={14} aria-hidden="true" />
              )}
              {settings.analyze ? t('wave.analysis.pause') : t('wave.analysis.resume')}
            </button>
          </div>
        </Row>
      </Group>

      <Group title={t('wave.online.title')}>
        <Row label={t('wave.links.label')} hint={t('wave.links.hint')} layout="inline">
          <Switch
            label={t('wave.links.label')}
            checked={settings.analyzeLinks}
            onChange={(analyzeLinks) => set({ analyzeLinks })}
          />
        </Row>
        {settings.analyzeLinks && status.linksTotal > 0 && (
          <span className={styles.hint}>
            {t('wave.links.count', { done: status.linksDone, total: status.linksTotal })}
            {linksText ? ` · ${linksText}` : ''}
          </span>
        )}
        <Row label={t('wave.radio.label')} hint={t('wave.radio.hint')} layout="inline">
          <Switch
            label={t('wave.radio.label')}
            checked={settings.radio}
            onChange={(radio) => set({ radio })}
          />
        </Row>
      </Group>
    </>
  );
}
