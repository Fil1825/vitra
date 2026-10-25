import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { library, type AlbumOrder } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { AlbumGrid } from './AlbumGrid.tsx';
import { LoadError, PageHeader } from './common.tsx';
import styles from './Library.module.css';
import { LibraryEmpty } from './LibraryEmpty.tsx';
import { WaveStartButton } from '../wave/WaveButtons.tsx';

/** All albums, or (`recent`) the newest additions first. */
export function AlbumsView({ recent = false }: { recent?: boolean }) {
  const { t } = useTranslation();
  const [chosen, setChosen] = useState<AlbumOrder>('artist');
  const order = recent ? 'recent' : chosen;
  const { data: albums, error } = useLibraryData(`albums:${order}`, () => library.albums(order));

  return (
    <div className={styles.view}>
      <PageHeader title={t(recent ? 'nav.recent' : 'nav.albums')}>
        <div className={styles.actions}>
          {albums !== undefined && albums.length > 0 && <WaveStartButton />}
          {!recent && (
            <SegmentedControl<AlbumOrder>
              label={t('nav.albums')}
              value={chosen}
              options={[
                { value: 'artist', label: t('library.sortArtist') },
                { value: 'recent', label: t('library.sortRecent') },
              ]}
              onChange={setChosen}
            />
          )}
        </div>
      </PageHeader>
      {error !== undefined && <LoadError error={error} />}
      {albums?.length === 0 ? <LibraryEmpty /> : <AlbumGrid albums={albums ?? []} />}
    </div>
  );
}
