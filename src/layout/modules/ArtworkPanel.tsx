import { useTranslation } from 'react-i18next';

import symbolUrl from '../../../assets/brand/symbol.svg';
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
  return (
    <Glass as="section" className={`${styles.artworkPanel} ${className ?? ''}`} priority={2}>
      {track?.artwork ? (
        <button
          type="button"
          className={styles.cover}
          aria-label={t('artwork.view')}
          onClick={() =>
            viewArtwork({
              source: { kind: 'track', path: track.path },
              preview: track.artwork ? artworkUrl(track.artwork.id) : null,
            })
          }
        >
          <img src={artworkUrl(track.artwork.id)} alt="" draggable={false} />
        </button>
      ) : (
        <div className={styles.cover}>
          <img className={styles.placeholder} src={symbolUrl} alt="" />
        </div>
      )}
      <div className={styles.caption}>
        <span className={styles.title}>{track?.title ?? t('player.notPlaying')}</span>
        {artist && <span className={styles.subtitle}>{artist}</span>}
      </div>
    </Glass>
  );
}
