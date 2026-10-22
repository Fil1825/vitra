import { describe, expect, it } from 'vitest';

import {
  formatOffset,
  formatStamp,
  lineAt,
  lineEnd,
  wordProgress,
  type Lyrics,
  type LyricsLine,
} from './lyrics.ts';

const line = (start: number | null, text: string, words: [number, string][] = []): LyricsLine => ({
  start,
  text,
  words: words.map(([wordStart, wordText]) => ({ start: wordStart, text: wordText })),
  translation: null,
});

const synced: Lyrics = {
  synced: true,
  offsetMs: 0,
  tags: [],
  lines: [line(1000, 'One'), line(4000, 'Two'), line(4000, 'Also two'), line(9000, '')],
};

describe('lineAt', () => {
  it('finds the last line that has started', () => {
    expect(lineAt(synced, 0)).toBeNull();
    expect(lineAt(synced, 999)).toBeNull();
    expect(lineAt(synced, 1000)).toBe(0);
    expect(lineAt(synced, 3999)).toBe(0);
    expect(lineAt(synced, 4000)).toBe(2);
    expect(lineAt(synced, 100_000)).toBe(3);
  });

  it('never points into plain lyrics', () => {
    expect(lineAt({ ...synced, synced: false }, 5000)).toBeNull();
  });
});

describe('lineEnd', () => {
  it('ends where the next line starts', () => {
    expect(lineEnd(synced, 0)).toBe(4000);
    expect(lineEnd(synced, 3)).toBe(14_000);
  });
});

describe('wordProgress', () => {
  const sung = line(1000, 'Hello big world', [
    [1000, 'Hello '],
    [1500, 'big '],
    [2000, 'world'],
  ]);

  it('follows the word being sung', () => {
    expect(wordProgress(sung, 4000, 900)).toEqual({ index: -1, fraction: 0 });
    expect(wordProgress(sung, 4000, 1250)).toEqual({ index: 0, fraction: 0.5 });
    expect(wordProgress(sung, 4000, 1500)).toEqual({ index: 1, fraction: 0 });
    expect(wordProgress(sung, 4000, 3000)).toEqual({ index: 2, fraction: 0.5 });
  });

  it('caps a held last word', () => {
    expect(wordProgress(sung, 60_000, 3500)).toEqual({ index: 2, fraction: 0.5 });
  });
});

describe('formatting', () => {
  it('shows offsets with a sign', () => {
    expect(formatOffset(0)).toBe('0.0 s');
    expect(formatOffset(500)).toBe('+0.5 s');
    expect(formatOffset(-1200)).toBe('−1.2 s');
  });

  it('writes LRC timestamps', () => {
    expect(formatStamp(0)).toBe('00:00.00');
    expect(formatStamp(83_456)).toBe('01:23.46');
    expect(formatStamp(600_000)).toBe('10:00.00');
  });
});
