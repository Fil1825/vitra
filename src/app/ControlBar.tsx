import { Pause, Play, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { IconButton } from '../components/ui/IconButton.tsx';
import { Slider } from '../components/ui/Slider.tsx';
import { formatTime } from '../lib/format.ts';
import { artworkUrl, player } from '../lib/player.ts';
import { useLivePosition, usePlayer } from '../stores/player.ts';
import styles from './ControlBar.module.css';
import { SpeedMenu } from './SpeedMenu.tsx';

const report = (err: unknown) => console.error('Player command failed', err);

export function ControlBar({ className }: { className?: string }) {
  const { t } = useTranslation();
  const state = usePlayer((s) => s.state);
  const position = useLivePosition();
  const [scrub, setScrub] = useState<number | null>(null);
  const [volume, setVolume] = useState<number | null>(null);

  const { track, duration } = state;
  const loaded = track !== null && state.status !== 'idle';
  const playing = state.status === 'playing' || state.status === 'loading';
  const shownPosition = scrub ?? position;
  const shownVolume = volume ?? state.volume;
  const VolumeIcon =
    state.muted || shownVolume === 0 ? VolumeX : shownVolume < 50 ? Volume1 : Volume2;
  const subtitle = [track?.metadata.artist, track?.metadata.album].filter(Boolean).join(' — ');

  return (
    <Glass
      as="footer"
      className={`${styles.bar} ${className ?? ''}`}
      aria-label={t('player.controls')}
      priority={3}
    >
      <div className={styles.track}>
        {track?.artwork ? (
          <img className={styles.artwork} src={artworkUrl(track.artwork.id)} alt="" />
        ) : (
          <div className={styles.artwork} aria-hidden="true" />
        )}
        <div className={styles.trackText}>
          <span className={styles.trackTitle}>{track?.title ?? t('player.notPlaying')}</span>
          {subtitle && <span className={styles.trackSubtitle}>{subtitle}</span>}
        </div>
      </div>

      <div className={styles.transport}>
        <div className={styles.buttons}>
          <IconButton
            label={t('player.previous')}
            disabled={!loaded}
            // Restarts the track; going to the previous one needs the queue (stage 3).
            onClick={() => player.seek(0).catch(report)}
          >
            <SkipBack size={18} fill="currentColor" />
          </IconButton>
          <IconButton
            label={playing ? t('player.pause') : t('player.play')}
            size="large"
            disabled={track === null}
            onClick={() => player.toggle().catch(report)}
          >
            {playing ? (
              <Pause size={22} fill="currentColor" />
            ) : (
              <Play size={22} fill="currentColor" />
            )}
          </IconButton>
          <IconButton label={t('player.next')} disabled>
            <SkipForward size={18} fill="currentColor" />
          </IconButton>
        </div>
        <div className={styles.progress}>
          <span className={styles.time}>{formatTime(shownPosition)}</span>
          <Slider
            label={t('player.progress')}
            value={shownPosition}
            max={duration ?? 0}
            step={0.1}
            disabled={!loaded || duration === null}
            valueText={t('player.positionOf', {
              position: formatTime(shownPosition),
              duration: formatTime(duration ?? 0),
            })}
            onChange={setScrub}
            onCommit={(value) => {
              player
                .seek(value)
                .catch(report)
                .finally(() => setScrub(null));
            }}
          />
          <span className={styles.time}>
            {duration === null ? '' : `-${formatTime(Math.max(0, duration - shownPosition))}`}
          </span>
        </div>
      </div>

      <div className={styles.volume}>
        <SpeedMenu speed={state.speed} onChange={(speed) => player.setSpeed(speed).catch(report)} />
        <IconButton
          label={state.muted ? t('player.unmute') : t('player.mute')}
          onClick={() => player.setMuted(!state.muted).catch(report)}
        >
          <VolumeIcon size={16} />
        </IconButton>
        <Slider
          className={styles.volumeSlider}
          label={t('player.volume')}
          value={shownVolume}
          max={100}
          step={1}
          onChange={(value) => {
            setVolume(value);
            player.setVolume(value).catch(report);
          }}
          onCommit={() => setVolume(null)}
        />
      </div>
    </Glass>
  );
}
