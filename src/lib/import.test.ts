import { describe, expect, it } from 'vitest';

import {
  canRefresh,
  displayName,
  isPlaylistFile,
  isServiceLink,
  looksLikeTrackList,
} from './import.ts';

describe('isServiceLink', () => {
  it('knows the services whose lists are imported', () => {
    expect(isServiceLink('https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M?si=1')).toBe(
      true,
    );
    expect(isServiceLink('spotify:album:1DFixLWuPkv3KT3TnV35m3')).toBe(true);
    expect(isServiceLink('https://spotify.link/AbCd')).toBe(true);
    expect(isServiceLink('https://music.apple.com/ru/album/discovery/697194953')).toBe(true);
    expect(isServiceLink('https://www.deezer.com/en/playlist/908622995')).toBe(true);
    expect(isServiceLink('https://deezer.page.link/xyz')).toBe(true);
    expect(isServiceLink('  https://music.yandex.ru/album/3389008  ')).toBe(true);
    expect(isServiceLink('https://vk.com/music/playlist/-1_2_abc')).toBe(true);
  });

  it('leaves playable links to the link dialog', () => {
    expect(isServiceLink('https://music.youtube.com/playlist?list=PL1')).toBe(false);
    expect(isServiceLink('https://soundcloud.com/a/b')).toBe(false);
    expect(isServiceLink('https://example.com/spotify.com')).toBe(false);
    expect(isServiceLink('Daft Punk - One More Time')).toBe(false);
    expect(isServiceLink('ftp://open.spotify.com/x')).toBe(false);
  });
});

describe('looksLikeTrackList', () => {
  it('takes several lines', () => {
    expect(looksLikeTrackList('Daft Punk - One More Time\nJustice - D.A.N.C.E.')).toBe(true);
    expect(looksLikeTrackList('\n\nArtist - Song\n\nOther - Track\n')).toBe(true);
  });

  it('leaves a single line or prose alone', () => {
    expect(looksLikeTrackList('Daft Punk - One More Time')).toBe(false);
    expect(looksLikeTrackList(`${'word '.repeat(80)}\n${'word '.repeat(80)}`)).toBe(false);
  });
});

describe('playlist files and sources', () => {
  it('knows playlist files', () => {
    expect(isPlaylistFile('/Users/me/Mix.m3u8')).toBe(true);
    expect(isPlaylistFile('C:\\Music\\Library.XML')).toBe(true);
    expect(isPlaylistFile('/tmp/spotify.csv')).toBe(true);
    expect(isPlaylistFile('/Users/me/song.mp3')).toBe(false);
    expect(isPlaylistFile('/Users/me/m3u')).toBe(false);
  });

  it('refreshes any imported source', () => {
    expect(canRefresh('https://open.spotify.com/playlist/x')).toBe(true);
    expect(canRefresh('music-app:playlist/0123456789ABCDEF')).toBe(true);
    expect(canRefresh(null)).toBe(false);
  });

  it('names a track artist first', () => {
    expect(displayName({ title: 'Song', artists: ['A', 'B'] })).toBe('A — Song');
    expect(displayName({ title: 'Song', artists: [] })).toBe('Song');
  });
});
