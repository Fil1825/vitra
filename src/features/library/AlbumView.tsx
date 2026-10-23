import { useTranslation } from 'react-i18next';

import { coverUrl, library } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { useUi } from '../../stores/ui.ts';
import { EmptyState, LoadError, PlayButtons } from './common.tsx';
import { HeroCover } from './HeroCover.tsx';
import styles from './Library.module.css';
import { useListSummary } from './summary.ts';
import type { Column } from './sort.ts';
import { TrackTable } from './TrackTable.tsx';

export function AlbumView({ id }: { id: number }) {
  const { t } = useTranslation();
  const go = useNavigation((s) => s.go);
  const viewArtwork = useUi((s) => s.viewArtwork);
  const summary = useListSummary();
  const album = useLibraryData(`album:${id}`, () => library.album(id));
  const tracks = useLibraryData(`album-tracks:${id}`, () => library.tracks({ kind: 'album', id }));

  if (album.data === null) {
    return <EmptyState title={t('library.nothingHere')} />;
  }
  const list = tracks.data ?? [];
  const data = album.data;
  // Compilations show who plays each track.
  const mixed = new Set(list.map((track) => track.artist)).size > 1;
  const columns: Column[] = mixed
    ? ['number', 'title', 'artist', 'time', 'favorite']
    : ['number', 'title', 'time', 'favorite'];

  return (
    <div className={styles.view}>
      {album.error !== undefined && <LoadError error={album.error} />}
      <header className={styles.hero}>
        <HeroCover
          target={{ kind: 'album', id }}
          cover={data?.cover ?? null}
          custom={data?.customCover ?? false}
          onView={() =>
            viewArtwork({
              source: { kind: 'album', id },
              preview: data?.cover ? coverUrl(data.cover) : null,
            })
          }
        />
        <div className={styles.heroText}>
          <span className={styles.kind}>{t('library.album')}</span>
          <h2 className={styles.heroTitle}>{data?.title}</h2>
          {data && (
            <p className={styles.heroSubtitle}>
              {data.artistId !== null ? (
                <button
                  type="button"
                  className={styles.link}
                  onClick={() =>
                    data.artistId !== null && go({ view: 'artist', id: data.artistId })
                  }
                >
                  {data.artist}
                </button>
              ) : (
                t('library.unknownArtist')
              )}
            </p>
          )}
          {data && (
            <p className={styles.heroMeta}>
              {[data.year, summary(data.trackCount, data.duration)].filter(Boolean).join(' · ')}
            </p>
          )}
          <PlayButtons ids={list.map((track) => track.id)} />
        </div>
      </header>
      <TrackTable tracks={list} columns={columns} />
    </div>
  );
}
