import type { LibraryTrack } from '../../lib/library.ts';

export type Column =
  'number' | 'title' | 'artist' | 'album' | 'time' | 'added' | 'plays' | 'favorite';

export type SortKey = Exclude<Column, 'number' | 'favorite'>;

export interface Sort {
  key: SortKey;
  descending: boolean;
}

function sortValue(track: LibraryTrack, key: SortKey): string | number | null {
  switch (key) {
    case 'title':
      return track.title;
    case 'artist':
      return track.artist;
    case 'album':
      return track.album;
    case 'time':
      return track.duration;
    case 'added':
      return track.addedAt;
    case 'plays':
      return track.playCount;
  }
}

/** Sorts a copy; missing values go last either way. */
export function sortTracks(tracks: readonly LibraryTrack[], sort: Sort, locale: string) {
  const collator = new Intl.Collator(locale, { numeric: true, sensitivity: 'base' });
  return [...tracks].sort((a, b) => {
    const x = sortValue(a, sort.key);
    const y = sortValue(b, sort.key);
    if (x === null || y === null) {
      return x === y ? 0 : x === null ? 1 : -1;
    }
    const order =
      typeof x === 'number' && typeof y === 'number'
        ? x - y
        : collator.compare(String(x), String(y));
    return sort.descending ? -order : order;
  });
}
