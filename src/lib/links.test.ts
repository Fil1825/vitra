import { describe, expect, it } from 'vitest';

import { isActive, isRemotePath, looksLikeLink } from './links.ts';

describe('links', () => {
  it('recognizes pasted links', () => {
    for (const text of [
      'https://www.youtube.com/watch?v=dQw4w9WgXcQ',
      '  https://soundcloud.com/a/b  ',
      'http://radio.example.com:8000/live.pls',
      'www.youtube.com/watch?v=x',
      'youtu.be/abc',
    ]) {
      expect(looksLikeLink(text), text).toBe(true);
    }
    for (const text of [
      '',
      'hello world',
      'Artist - Title',
      'music.flac',
      'https://x y',
      'localhost',
    ]) {
      expect(looksLikeLink(text), text).toBe(false);
    }
  });

  it('tells links from files', () => {
    expect(isRemotePath('https://youtu.be/x')).toBe(true);
    expect(isRemotePath('HTTP://example.com/a.mp3')).toBe(true);
    expect(isRemotePath('/Users/me/Music/a.flac')).toBe(false);
    expect(isRemotePath('C:\\Music\\a.flac')).toBe(false);
  });

  it('knows which downloads are still going', () => {
    expect(isActive({ state: 'queued' })).toBe(true);
    expect(isActive({ state: 'downloading', progress: 0.5 })).toBe(true);
    expect(isActive({ state: 'processing' })).toBe(true);
    expect(isActive({ state: 'done', path: '/a.opus' })).toBe(false);
    expect(isActive({ state: 'failed', error: 'x' })).toBe(false);
    expect(isActive({ state: 'cancelled' })).toBe(false);
  });
});
