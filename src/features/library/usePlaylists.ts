import { playlists, type Playlist } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';

const PLAYLIST_CHANGES = ['playlists'] as const;

/** All playlists, kept fresh. */
export function usePlaylists(): Playlist[] {
  return useLibraryData('playlists', playlists.list, PLAYLIST_CHANGES).data ?? [];
}
