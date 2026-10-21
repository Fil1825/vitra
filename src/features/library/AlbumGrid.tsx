import { Play } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { library, type Album, type Artist } from '../../lib/library.ts';
import { showContextMenu } from '../../lib/menu.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { albumMenu, playTracks, report } from './actions.ts';
import styles from './AlbumGrid.module.css';
import { Cover } from './Cover.tsx';
import { usePlaylists } from './usePlaylists.ts';

export function AlbumGrid({
  albums,
  showArtist = true,
}: {
  albums: Album[];
  showArtist?: boolean;
}) {
  const { t } = useTranslation();
  const go = useNavigation((s) => s.go);
  const allPlaylists = usePlaylists();

  return (
    <ul className={styles.grid}>
      {albums.map((album) => (
        <li key={album.id} className={styles.tile}>
          <div
            className={styles.art}
            onContextMenu={(event) => {
              event.preventDefault();
              showContextMenu(albumMenu(album, allPlaylists)).catch(report('Showing a menu'));
            }}
          >
            <button
              type="button"
              className={styles.open}
              aria-label={album.title}
              onClick={() => go({ view: 'album', id: album.id })}
            >
              <Cover id={album.cover} />
            </button>
            <button
              type="button"
              className={styles.play}
              aria-label={`${t('library.play')}: ${album.title}`}
              onClick={() =>
                library
                  .tracks({ kind: 'album', id: album.id })
                  .then((tracks) => playTracks(tracks.map((track) => track.id)))
                  .catch(report('Playing an album'))
              }
            >
              <Play size={16} fill="currentColor" />
            </button>
          </div>
          <span className={styles.title} title={album.title}>
            {album.title}
          </span>
          {showArtist ? (
            <span className={styles.subtitle}>{album.artist ?? t('library.unknownArtist')}</span>
          ) : (
            album.year !== null && <span className={styles.subtitle}>{album.year}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

export function ArtistGrid({ artists }: { artists: Artist[] }) {
  const { t } = useTranslation();
  const go = useNavigation((s) => s.go);
  return (
    <ul className={styles.grid}>
      {artists.map((artist) => (
        <li key={artist.id} className={`${styles.tile} ${styles.artist}`}>
          <button
            type="button"
            className={styles.open}
            aria-label={artist.name}
            onClick={() => go({ view: 'artist', id: artist.id })}
          >
            <Cover id={artist.cover} shape="round" />
          </button>
          <span className={styles.title} title={artist.name}>
            {artist.name}
          </span>
          <span className={styles.subtitle}>
            {artist.albumCount > 0
              ? t('library.albumCount', { count: artist.albumCount })
              : t('library.songCount', { count: artist.trackCount })}
          </span>
        </li>
      ))}
    </ul>
  );
}
