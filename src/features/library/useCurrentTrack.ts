import { library, type LibraryTrack } from '../../lib/library.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { usePlayer } from '../../stores/player.ts';

/** The library record of the playing track (album and artist ids, favorite), if it has one. */
export function useCurrentLibraryTrack(): LibraryTrack | undefined {
  const id = usePlayer((s) => s.state.track?.libraryId ?? null);
  const { data } = useLibraryData(id === null ? null : `track:${id}`, () =>
    id === null ? Promise.resolve(null) : library.track(id),
  );
  return data ?? undefined;
}
