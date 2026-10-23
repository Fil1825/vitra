import { describe, expect, it } from 'vitest';

import type { Album } from '../../lib/library.ts';
import { albumTarget } from './album.ts';

const album = (id: number, title: string, artist: string | null): Album => ({
  id,
  title,
  artist,
  artistId: null,
  year: null,
  cover: null,
  customCover: false,
  trackCount: 1,
  linkCount: 0,
  duration: 0,
  addedAt: 0,
});

describe('albumTarget', () => {
  const albums = [album(1, 'Real Album', 'Real Artist'), album(2, 'Hits', 'Other')];

  it('finds the album by title and album artist, else the first artist', () => {
    expect(
      albumTarget({ album: 'real album', albumArtist: '', artist: 'Real Artist; Guest' }, albums),
    ).toEqual(albums[0]);
    expect(albumTarget({ album: 'Hits', albumArtist: 'Other', artist: 'Someone' }, albums)).toEqual(
      albums[1],
    );
  });

  it('makes a new album when no album matches both', () => {
    expect(albumTarget({ album: 'Hits', albumArtist: '', artist: 'Someone' }, albums)).toBe('new');
    expect(albumTarget({ album: 'Singles', albumArtist: '', artist: 'A' }, albums)).toBe('new');
  });

  it('is no album without a title', () => {
    expect(albumTarget({ album: '  ', albumArtist: 'A', artist: 'A' }, albums)).toBeNull();
  });
});
