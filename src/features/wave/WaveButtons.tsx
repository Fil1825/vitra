import { Heart, ThumbsDown, Waves } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { library } from '../../lib/library.ts';
import { sameMode } from '../../lib/wave.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useQueue } from '../../stores/queue.ts';
import { report } from '../library/actions.ts';
import libraryStyles from '../library/Library.module.css';
import { dislikeTrack, startWave, useWaveUi } from './actions.ts';

interface ButtonProps {
  trackId: number;
  className?: string;
  /** Lucide's size: pixels, or `1em` where the window scales everything (the mini player). */
  size?: number | string;
}

/** The heart: a favorite, and the wave's "like" while one plays. */
export function LikeButton({
  trackId,
  favorite,
  className,
  onClassName,
  size = 16,
}: ButtonProps & { favorite: boolean; onClassName?: string }) {
  const { t } = useTranslation();
  const inWave = useQueue((s) => s.queue.wave !== null);
  const label = favorite ? t('player.unfavorite') : inWave ? t('wave.like') : t('player.favorite');
  return (
    <IconButton
      label={label}
      aria-pressed={favorite}
      className={[className, favorite && onClassName].filter(Boolean).join(' ') || undefined}
      onClick={() => library.setFavorite(trackId, !favorite).catch(report('Favoriting'))}
    >
      <Heart size={size} fill={favorite ? 'currentColor' : 'none'} />
    </IconButton>
  );
}

/** "Don't like", while a wave plays: the wave leaves the track out and moves on. */
export function DislikeButton({ trackId, className, size = 16 }: ButtonProps) {
  const { t } = useTranslation();
  const inWave = useQueue((s) => s.queue.wave !== null);
  if (!inWave) {
    return null;
  }
  return (
    <IconButton
      label={t('wave.dislike')}
      className={className}
      onClick={() => dislikeTrack(trackId)}
    >
      <ThumbsDown size={size} />
    </IconButton>
  );
}

/** Starts a wave like this track (the track goes on playing). */
export function WaveFromTrackButton({ trackId, className, size = 16 }: ButtonProps) {
  const { t } = useTranslation();
  const mode = useQueue((s) => s.queue.wave);
  const starting = useWaveUi((s) => s.starting !== null);
  const own = { kind: 'likeTrack', track: trackId } as const;
  if (sameMode(mode, own)) {
    return null;
  }
  return (
    <IconButton
      label={t('wave.fromTrack')}
      className={className}
      disabled={starting}
      onClick={() => void startWave(own)}
    >
      <Waves size={size} />
    </IconButton>
  );
}

/** "My Wave · Calm" while a wave plays: opens the wave's page. */
export function WaveBadge({ className }: { className?: string }) {
  const { t } = useTranslation();
  const mode = useQueue((s) => s.queue.wave);
  const go = useNavigation((s) => s.go);
  if (mode === null) {
    return null;
  }
  return (
    <button type="button" className={className} onClick={() => go({ view: 'wave' })}>
      <Waves size={13} aria-hidden="true" />
      {t('wave.badge', { mood: t(`wave.moods.${mode.kind}`) })}
    </button>
  );
}

/** "My Wave" in the library's headers: starts the wave to your taste, or opens a running one. */
export function WaveStartButton() {
  const { t } = useTranslation();
  const active = useQueue((s) => s.queue.wave !== null);
  const starting = useWaveUi((s) => s.starting !== null);
  const go = useNavigation((s) => s.go);
  return (
    <button
      type="button"
      className={libraryStyles.secondaryButton}
      disabled={starting}
      onClick={() => (active ? go({ view: 'wave' }) : void startWave({ kind: 'taste' }))}
    >
      <Waves size={15} aria-hidden="true" />
      {t('wave.title')}
    </button>
  );
}
