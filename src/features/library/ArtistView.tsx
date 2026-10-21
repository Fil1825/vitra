import { useTranslation } from 'react-i18next';

import { library } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { AlbumGrid } from './AlbumGrid.tsx';
import { EmptyState, LoadError, PlayButtons } from './common.tsx';
import { Cover } from './Cover.tsx';
import styles from './Library.module.css';
import { TrackTable } from './TrackTable.tsx';

export function ArtistView({ id }: { id: number }) {
  const { t } = useTranslation();
  const artist = useLibraryData(`artist:${id}`, () => library.artist(id));
  const albums = useLibraryData(`artist-albums:${id}`, () => library.artistAlbums(id));
  const tracks = useLibraryData(`artist-tracks:${id}`, () =>
    library.tracks({ kind: 'artist', id }),
  );

  if (artist.data === null) {
    return <EmptyState title={t('library.nothingHere')} />;
  }
  const data = artist.data;
  const list = tracks.data ?? [];

  return (
    <div className={styles.view}>
      {artist.error !== undefined && <LoadError error={artist.error} />}
      <header className={styles.hero}>
        <Cover id={data?.cover ?? null} shape="round" className={styles.heroCover} />
        <div className={styles.heroText}>
          <span className={styles.kind}>{t('library.artist')}</span>
          <h2 className={styles.heroTitle}>{data?.name}</h2>
          {data && (
            <p className={styles.heroMeta}>
              {[
                data.albumCount > 0 && t('library.albumCount', { count: data.albumCount }),
                t('library.songCount', { count: list.length || data.trackCount }),
              ]
                .filter(Boolean)
                .join(', ')}
            </p>
          )}
          <PlayButtons ids={list.map((track) => track.id)} />
        </div>
      </header>
      {(albums.data?.length ?? 0) > 0 && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('library.albumsSection')}</h3>
          <AlbumGrid albums={albums.data ?? []} showArtist={false} />
        </section>
      )}
      {list.length > 0 && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('library.songsSection')}</h3>
          <TrackTable
            tracks={list}
            columns={['title', 'album', 'time', 'favorite']}
            covers
            sortable
          />
        </section>
      )}
    </div>
  );
}
