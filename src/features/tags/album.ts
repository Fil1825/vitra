import type { Album } from '../../lib/library.ts';

/** The fields that decide a track's album. */
export interface AlbumFields {
  album: string;
  albumArtist: string;
  /** Artists separated by ";". */
  artist: string;
}

/**
 * Where the fields put the track: an album already in the library, a new one, or none (no
 * album title). The library knows an album by its title and album artist, or the first
 * artist when there is no album artist.
 */
export function albumTarget(fields: AlbumFields, albums: readonly Album[]): Album | 'new' | null {
  const title = fields.album.trim().toLowerCase();
  if (!title) {
    return null;
  }
  const artist = (
    fields.albumArtist.trim() ||
    fields.artist.split(';')[0]?.trim() ||
    ''
  ).toLowerCase();
  return (
    albums.find(
      (album) =>
        album.title.toLowerCase() === title && (album.artist ?? '').toLowerCase() === artist,
    ) ?? 'new'
  );
}
