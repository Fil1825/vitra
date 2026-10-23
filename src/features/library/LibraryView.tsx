import { NowPlaying } from '../../app/NowPlaying.tsx';
import { useLyrics } from '../../stores/lyrics.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { usePlayer } from '../../stores/player.ts';
import { useUi } from '../../stores/ui.ts';
import { DownloadsView } from '../links/DownloadsView.tsx';
import { LyricsView } from '../lyrics/LyricsView.tsx';
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
      return <NowPlayingStage />;
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

/** The playing track, with its lyrics beside it when it has some (and the panel doesn't). */
function NowPlayingStage() {
  const trackId = usePlayer((s) => s.state.track?.id ?? null);
  const hasLyrics = useLyrics((s) => s.current.trackId === trackId && s.current.status === 'ready');
  const panelShowsLyrics = useUi((s) => s.sidePanel === 'lyrics');
  const showLyrics = trackId !== null && hasLyrics && !panelShowsLyrics;
  return (
    <div className={styles.nowPlaying} data-lyrics={showLyrics || undefined}>
      <NowPlaying />
      {showLyrics && <LyricsView variant="stage" className={styles.stageLyrics} />}
    </div>
  );
}
