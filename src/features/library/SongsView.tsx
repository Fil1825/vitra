import { Heart } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { library } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { EmptyState, LoadError, PageHeader, PlayButtons } from './common.tsx';
import styles from './Library.module.css';
import { useListSummary } from './summary.ts';
import { LibraryEmpty } from './LibraryEmpty.tsx';
import { TrackTable } from './TrackTable.tsx';

/** Every song in a sortable table, or (`favorites`) just the favorites. */
export function SongsView({ favorites = false }: { favorites?: boolean }) {
  const { t } = useTranslation();
  const summary = useListSummary();
  const kind = favorites ? 'favorites' : 'all';
  const { data, error } = useLibraryData(`tracks:${kind}`, () => library.tracks({ kind }));
  const tracks = data ?? [];
  const total = tracks.reduce((sum, track) => sum + (track.duration ?? 0), 0);

  return (
    <div className={styles.view}>
      <PageHeader title={t(favorites ? 'nav.favorites' : 'nav.songs')}>
        {tracks.length > 0 && (
          <div className={styles.actions}>
            <span className={styles.scanNote}>{summary(tracks.length, total)}</span>
            <PlayButtons ids={tracks.map((track) => track.id)} />
          </div>
        )}
      </PageHeader>
      {error !== undefined && <LoadError error={error} />}
      {data?.length === 0 ? (
        favorites ? (
          <EmptyState
            icon={<Heart size={36} aria-hidden="true" />}
            title={t('library.noFavorites')}
            body={t('library.noFavoritesBody')}
          />
        ) : (
          <LibraryEmpty />
        )
      ) : (
        <TrackTable
          tracks={tracks}
          columns={['title', 'artist', 'album', 'time', 'favorite']}
          covers
          sortable
        />
      )}
    </div>
  );
}
