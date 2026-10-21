import { AlertTriangle, FolderOpen } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import symbolUrl from '../../assets/brand/symbol.svg';
import { Glass } from '../components/Glass/index.ts';
import { LibraryLink } from '../features/library/TrackLinks.tsx';
import { useCurrentLibraryTrack } from '../features/library/useCurrentTrack.ts';
import { pickAndPlay } from '../lib/openFiles.ts';
import { artworkUrl, type PlayerError, type TrackMetadata } from '../lib/player.ts';
import { albumRoute, artistRoute } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import styles from './NowPlaying.module.css';

/** "FLAC · 44.1 kHz · 24-bit" — whatever is known. */
function technicalLine(m: TrackMetadata): string {
  return [
    m.format,
    m.sampleRate && `${Number((m.sampleRate / 1000).toFixed(1))} kHz`,
    m.bitDepth && `${m.bitDepth}-bit`,
    !m.bitDepth && m.bitrate && `${m.bitrate} kbps`,
  ]
    .filter(Boolean)
    .join(' · ');
}

function ErrorBanner({ error }: { error: PlayerError }) {
  const { t } = useTranslation();
  const message =
    error.kind === 'mpvMissing'
      ? t('player.errors.mpvMissing')
      : error.kind === 'mpvFailed'
        ? t('player.errors.mpvFailed', { message: error.message })
        : t('player.errors.playback', { message: error.message });
  return (
    <p className={styles.error} role="alert">
      <AlertTriangle size={16} aria-hidden="true" />
      <span>
        {message}
        {error.kind === 'mpvMissing' && (
          <span className={styles.errorHint}>{t('player.errors.mpvMissingHint')}</span>
        )}
      </span>
    </p>
  );
}

export function NowPlaying() {
  const { t } = useTranslation();
  const { track, error } = usePlayer((s) => s.state);
  const libraryTrack = useCurrentLibraryTrack();

  if (!track) {
    return (
      <Glass as="section" className={styles.empty} priority={1}>
        {error && <ErrorBanner error={error} />}
        <img className={styles.symbol} src={symbolUrl} alt="" width={168} height={168} />
        <h2 className={styles.emptyTitle}>{t('nowPlaying.emptyTitle')}</h2>
        <p className={styles.emptyBody}>{t('nowPlaying.emptyBody')}</p>
        <button type="button" className={styles.openButton} onClick={() => void pickAndPlay()}>
          <FolderOpen size={16} aria-hidden="true" />
          {t('player.open')}
        </button>
      </Glass>
    );
  }

  const { metadata } = track;
  const artist = metadata.artist ?? metadata.albumArtist;
  const technical = technicalLine(metadata);

  return (
    <section className={styles.track} aria-label={t('nowPlaying.label')}>
      {error && <ErrorBanner error={error} />}
      <div className={styles.cover}>
        {track.artwork ? (
          <img
            key={track.artwork.id}
            className={styles.coverImage}
            src={artworkUrl(track.artwork.id)}
            alt=""
          />
        ) : (
          <img className={styles.coverPlaceholder} src={symbolUrl} alt="" />
        )}
      </div>
      <h2 className={styles.title}>
        <LibraryLink to={albumRoute(libraryTrack?.albumId)}>{track.title}</LibraryLink>
      </h2>
      {(artist || metadata.album) && (
        <p className={styles.byline}>
          {artist && <LibraryLink to={artistRoute(libraryTrack?.artistId)}>{artist}</LibraryLink>}
          {artist && metadata.album && ' — '}
          {metadata.album && (
            <LibraryLink to={albumRoute(libraryTrack?.albumId)}>{metadata.album}</LibraryLink>
          )}
        </p>
      )}
      {technical && <p className={styles.technical}>{technical}</p>}
    </section>
  );
}
