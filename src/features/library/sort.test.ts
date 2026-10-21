import { describe, expect, it } from 'vitest';

import type { LibraryTrack } from '../../lib/library.ts';
import { sortTracks } from './sort.ts';

function track(id: number, title: string, extra: Partial<LibraryTrack> = {}): LibraryTrack {
  return {
    id,
    path: `/${id}.flac`,
    title,
    artist: null,
    artistId: null,
    album: null,
    albumId: null,
    albumArtist: null,
    genre: null,
    year: null,
    trackNumber: null,
    discNumber: null,
    duration: null,
    format: null,
    sampleRate: null,
    bitDepth: null,
    bitrate: null,
    favorite: false,
    playCount: 0,
    lastPlayedAt: null,
    addedAt: 0,
    cover: null,
    folderId: 1,
    ...extra,
  };
}

const titles = (tracks: LibraryTrack[]) => tracks.map((t) => t.title);

describe('sortTracks', () => {
  const tracks = [
    track(1, 'track 10', { duration: 30 }),
    track(2, 'Ёлка', { artist: 'Б' }),
    track(3, 'track 2', { duration: 10, artist: 'а' }),
    track(4, 'Apple', { duration: 20 }),
  ];

  it('sorts text naturally and case-insensitively', () => {
    expect(titles(sortTracks(tracks, { key: 'title', descending: false }, 'en'))).toEqual([
      'Apple',
      'track 2',
      'track 10',
      'Ёлка',
    ]);
  });

  it('keeps missing values last in both directions', () => {
    const ascending = sortTracks(tracks, { key: 'time', descending: false }, 'en');
    expect(titles(ascending)).toEqual(['track 2', 'Apple', 'track 10', 'Ёлка']);
    const descending = sortTracks(tracks, { key: 'time', descending: true }, 'en');
    expect(titles(descending)).toEqual(['track 10', 'Apple', 'track 2', 'Ёлка']);
    const byArtist = sortTracks(tracks, { key: 'artist', descending: false }, 'ru');
    expect(titles(byArtist).slice(0, 2)).toEqual(['track 2', 'Ёлка']);
  });

  it('does not touch the input', () => {
    const copy = [...tracks];
    sortTracks(tracks, { key: 'title', descending: true }, 'en');
    expect(tracks).toEqual(copy);
  });
});
