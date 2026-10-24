import { useTranslation } from 'react-i18next';

import { CoverArt, VinylToggle } from '../../components/CoverArt/index.ts';
import { Glass } from '../../components/Glass/index.ts';
import { artworkUrl } from '../../lib/player.ts';
import { usePlayer } from '../../stores/player.ts';
import { useUi } from '../../stores/ui.ts';
import styles from './modules.module.css';

/** The big artwork module: the playing track's cover with its title, in a column. */
export function ArtworkPanel({ className }: { className?: string }) {
  const { t } = useTranslation();
  const track = usePlayer((s) => s.state.track);
  const viewArtwork = useUi((s) => s.viewArtwork);
  const artist = track?.metadata.artist ?? track?.metadata.albumArtist;
  const artwork = track?.artwork ?? null;
  return (
    <Glass as="section" className={`${styles.artworkPanel} ${className ?? ''}`} priority={2}>
      <div className={styles.coverPlace}>
        {track && artwork ? (
          <button
            type="button"
            className={styles.coverButton}
            aria-label={t('artwork.view')}
            onClick={() =>
              viewArtwork({
                source: { kind: 'track', path: track.path },
                preview: artworkUrl(artwork.id),
              })
            }
          >
            <CoverArt src={artworkUrl(artwork.id)} live className={styles.cover} />
          </button>
        ) : (
          <CoverArt src={null} live className={styles.cover} />
        )}
        <VinylToggle />
      </div>
      <div className={styles.caption}>
        <span className={styles.title}>{track?.title ?? t('player.notPlaying')}</span>
        {artist && <span className={styles.subtitle}>{artist}</span>}
      </div>
    </Glass>
  );
}
