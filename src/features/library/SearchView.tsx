import { Search } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { library } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { AlbumGrid, ArtistGrid } from './AlbumGrid.tsx';
import { EmptyState, LoadError, PageHeader } from './common.tsx';
import styles from './Library.module.css';
import { TrackTable } from './TrackTable.tsx';

const TRACK_LIMIT = 200;

export function SearchView({ query }: { query: string }) {
  const { t } = useTranslation();
  const trimmed = query.trim();
  const { data, error } = useLibraryData(trimmed ? `search:${trimmed}` : null, () =>
    library.search(trimmed, TRACK_LIMIT),
  );

  if (!trimmed) {
    return (
      <div className={styles.view}>
        <EmptyState
          icon={<Search size={36} aria-hidden="true" />}
          title={t('nav.search')}
          body={t('library.searchHint')}
        />
      </div>
    );
  }
  const empty =
    data !== undefined &&
    data.tracks.length === 0 &&
    data.albums.length === 0 &&
    data.artists.length === 0;

  return (
    <div className={styles.view}>
      <PageHeader title={`“${trimmed}”`} />
      {error !== undefined && <LoadError error={error} />}
      {empty && <EmptyState title={t('library.noResults', { query: trimmed })} />}
      {data && data.artists.length > 0 && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('library.artistsSection')}</h3>
          <ArtistGrid artists={data.artists} />
        </section>
      )}
      {data && data.albums.length > 0 && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('library.albumsSection')}</h3>
          <AlbumGrid albums={data.albums} />
        </section>
      )}
      {data && data.tracks.length > 0 && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('library.songsSection')}</h3>
          <TrackTable
            tracks={data.tracks}
            columns={['title', 'artist', 'album', 'time', 'favorite']}
            covers
          />
        </section>
      )}
    </div>
  );
}
