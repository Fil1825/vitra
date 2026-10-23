import {
  AlertTriangle,
  Heart,
  ListMusic,
  LoaderCircle,
  MicVocal,
  Pause,
  Play,
  Repeat,
  Repeat1,
  Shuffle,
  SkipBack,
  SkipForward,
  Volume1,
  Volume2,
  VolumeX,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { IconButton } from '../components/ui/IconButton.tsx';
import { Slider } from '../components/ui/Slider.tsx';
import { formatTime } from '../lib/format.ts';
import { library } from '../lib/library.ts';
import { artworkUrl, player } from '../lib/player.ts';
import { hasNext, nextRepeat, queue } from '../lib/queue.ts';
import { ArtistLinks, LibraryLink } from '../features/library/TrackLinks.tsx';
import { useCurrentLibraryTrack } from '../features/library/useCurrentTrack.ts';
import { albumRoute, useNavigation } from '../stores/navigation.ts';
import { useLivePosition, usePlayer } from '../stores/player.ts';
import { useQueue } from '../stores/queue.ts';
import { useUi } from '../stores/ui.ts';
import styles from './ControlBar.module.css';
import { playerErrorText } from './playerError.ts';
import { SpeedMenu } from './SpeedMenu.tsx';

const report = (err: unknown) => console.error('Player command failed', err);

/** Heart for the playing track, if it is in the library. */
function FavoriteButton({ trackId, favorite }: { trackId: number; favorite: boolean }) {
  const { t } = useTranslation();
  return (
    <IconButton
      label={t(favorite ? 'player.unfavorite' : 'player.favorite')}
      aria-pressed={favorite}
      className={favorite ? styles.on : undefined}
      onClick={() => library.setFavorite(trackId, !favorite).catch(report)}
    >
      <Heart size={16} fill={favorite ? 'currentColor' : 'none'} />
    </IconButton>
  );
}

export function ControlBar({ className }: { className?: string }) {
  const { t } = useTranslation();
  const state = usePlayer((s) => s.state);
  const queueState = useQueue((s) => s.queue);
  const sidePanel = useUi((s) => s.sidePanel);
  const togglePanel = useUi((s) => s.togglePanel);
  const viewArtwork = useUi((s) => s.viewArtwork);
  const queueOpen = sidePanel === 'queue';
  const lyricsOpen = sidePanel === 'lyrics';
  const go = useNavigation((s) => s.go);
  const libraryTrack = useCurrentLibraryTrack();
  const position = useLivePosition();
  const [scrub, setScrub] = useState<number | null>(null);
  const [volume, setVolume] = useState<number | null>(null);

  const { track, duration } = state;
  // Seeking needs an open file; a link can take a while to open (yt-dlp asks the site first).
  const seekable = track !== null && (state.status === 'playing' || state.status === 'paused');
  const loading = track !== null && state.status === 'loading';
  const playing = state.status === 'playing' || loading;
  const shownPosition = scrub ?? position;
  const shownVolume = volume ?? state.volume;
  const VolumeIcon =
    state.muted || shownVolume === 0 ? VolumeX : shownVolume < 50 ? Volume1 : Volume2;
  const subtitle = [track?.metadata.artist, track?.metadata.album].filter(Boolean).join(' — ');
  const repeatLabel = {
    off: t('player.repeatOff'),
    all: t('player.repeatAll'),
    one: t('player.repeatOne'),
  }[queueState.repeat];

  return (
    <Glass
      as="footer"
      className={`${styles.bar} ${className ?? ''}`}
      aria-label={t('player.controls')}
      priority={3}
    >
      <div className={styles.track}>
        {track?.artwork ? (
          <button
            type="button"
            className={styles.artworkButton}
            aria-label={t('artwork.view')}
            title={t('artwork.view')}
            onClick={() =>
              track.artwork &&
              viewArtwork({
                source: { kind: 'track', path: track.path },
                preview: artworkUrl(track.artwork.id),
              })
            }
          >
            <img className={styles.artwork} src={artworkUrl(track.artwork.id)} alt="" />
          </button>
        ) : (
          <button
            type="button"
            className={styles.artworkButton}
            aria-label={t('nav.nowPlaying')}
            onClick={() => go({ view: 'nowPlaying' })}
          >
            <div className={styles.artwork} aria-hidden="true" />
          </button>
        )}
        <div className={styles.trackText}>
          <LibraryLink to={albumRoute(libraryTrack?.albumId)} className={styles.trackTitle}>
            {track?.title ?? t('player.notPlaying')}
          </LibraryLink>
          {state.error ? (
            // In place of the artist: a link that stopped playback says why, wherever the
            // user is (Now Playing shows it in full).
            <button
              type="button"
              className={styles.trackError}
              role="alert"
              title={playerErrorText(state.error, t)}
              onClick={() => go({ view: 'nowPlaying' })}
            >
              <AlertTriangle size={12} aria-hidden="true" />
              <span>{playerErrorText(state.error, t)}</span>
            </button>
          ) : subtitle ? (
            <span className={styles.trackSubtitle}>
              {track?.metadata.artist ? (
                <ArtistLinks artists={libraryTrack?.artists} fallback={track.metadata.artist} />
              ) : null}
              {track?.metadata.artist && track.metadata.album ? ' — ' : null}
              {track?.metadata.album ? (
                <LibraryLink to={albumRoute(libraryTrack?.albumId)}>
                  {track.metadata.album}
                </LibraryLink>
              ) : null}
            </span>
          ) : null}
        </div>
        {libraryTrack && (
          <FavoriteButton trackId={libraryTrack.id} favorite={libraryTrack.favorite} />
        )}
      </div>

      <div className={styles.transport}>
        <div className={styles.buttons}>
          <IconButton
            label={t('player.shuffle')}
            aria-pressed={queueState.shuffle}
            className={queueState.shuffle ? styles.on : undefined}
            onClick={() => queue.setShuffle(!queueState.shuffle).catch(report)}
          >
            <Shuffle size={16} />
          </IconButton>
          <IconButton
            label={t('player.previous')}
            disabled={track === null}
            onClick={() => player.previous().catch(report)}
          >
            <SkipBack size={18} fill="currentColor" />
          </IconButton>
          <IconButton
            label={loading ? t('player.loading') : playing ? t('player.pause') : t('player.play')}
            size="large"
            disabled={track === null}
            aria-busy={loading || undefined}
            onClick={() => player.toggle().catch(report)}
          >
            {loading ? (
              <LoaderCircle size={22} className={styles.spin} />
            ) : playing ? (
              <Pause size={22} fill="currentColor" />
            ) : (
              <Play size={22} fill="currentColor" />
            )}
          </IconButton>
          <IconButton
            label={t('player.next')}
            disabled={!hasNext(queueState)}
            onClick={() => player.next().catch(report)}
          >
            <SkipForward size={18} fill="currentColor" />
          </IconButton>
          <IconButton
            label={repeatLabel}
            aria-pressed={queueState.repeat !== 'off'}
            className={queueState.repeat !== 'off' ? styles.on : undefined}
            onClick={() => queue.setRepeat(nextRepeat(queueState.repeat)).catch(report)}
          >
            {queueState.repeat === 'one' ? <Repeat1 size={16} /> : <Repeat size={16} />}
          </IconButton>
        </div>
        <div className={styles.progress}>
          <span className={styles.time}>{formatTime(shownPosition)}</span>
          <Slider
            label={t('player.progress')}
            value={shownPosition}
            max={duration ?? 0}
            step={0.1}
            disabled={!seekable || duration === null}
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
        <IconButton
          label={t(lyricsOpen ? 'player.hideLyrics' : 'player.showLyrics')}
          aria-pressed={lyricsOpen}
          className={lyricsOpen ? styles.on : undefined}
          onClick={() => togglePanel('lyrics')}
        >
          <MicVocal size={17} />
        </IconButton>
        <IconButton
          label={t(queueOpen ? 'player.hideQueue' : 'player.showQueue')}
          aria-pressed={queueOpen}
          className={queueOpen ? styles.on : undefined}
          onClick={() => togglePanel('queue')}
        >
          <ListMusic size={17} />
        </IconButton>
      </div>
    </Glass>
  );
}
