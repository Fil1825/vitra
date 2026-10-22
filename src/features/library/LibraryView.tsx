import { NowPlaying } from '../../app/NowPlaying.tsx';
import { useNavigation } from '../../stores/navigation.ts';
import { DownloadsView } from '../links/DownloadsView.tsx';
import { AlbumsView } from './AlbumsView.tsx';
import { AlbumView } from './AlbumView.tsx';
import { ArtistsView } from './ArtistsView.tsx';
import { ArtistView } from './ArtistView.tsx';
import styles from './Library.module.css';
import { PlaylistView } from './PlaylistView.tsx';
import { SearchView } from './SearchView.tsx';
import { SongsView } from './SongsView.tsx';

/** The main area: whatever the navigation points at. */
export function LibraryView() {
  const route = useNavigation((s) => s.route);
  switch (route.view) {
    case 'nowPlaying':
      return (
        <div className={styles.nowPlaying}>
          <NowPlaying />
        </div>
      );
    case 'recent':
      return <AlbumsView recent />;
    case 'albums':
      return <AlbumsView />;
    case 'album':
      return <AlbumView key={route.id} id={route.id} />;
    case 'artists':
      return <ArtistsView />;
    case 'artist':
      return <ArtistView key={route.id} id={route.id} />;
    case 'songs':
      return <SongsView />;
    case 'favorites':
      return <SongsView key="favorites" favorites />;
    case 'playlist':
      return <PlaylistView key={route.id} id={route.id} />;
    case 'downloads':
      return <DownloadsView />;
    case 'search':
      return <SearchView query={route.query} />;
  }
}
