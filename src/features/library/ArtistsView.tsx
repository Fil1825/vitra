import { useTranslation } from 'react-i18next';

import { library } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { ArtistGrid } from './AlbumGrid.tsx';
import { LoadError, PageHeader } from './common.tsx';
import styles from './Library.module.css';
import { LibraryEmpty } from './LibraryEmpty.tsx';

export function ArtistsView() {
  const { t } = useTranslation();
  const { data: artists, error } = useLibraryData('artists', library.artists);
  return (
    <div className={styles.view}>
      <PageHeader title={t('nav.artists')} />
      {error !== undefined && <LoadError error={error} />}
      {artists?.length === 0 ? <LibraryEmpty /> : <ArtistGrid artists={artists ?? []} />}
    </div>
  );
}
