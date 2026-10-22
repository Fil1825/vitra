import { describe, expect, it } from 'vitest';

import type { LyricsLine } from '../../lib/lyrics.ts';
import { nextLine, shiftAll, startSync, syncedLyrics, tap, undo } from './sync.ts';

const line = (text: string, start: number | null = null): LyricsLine => ({
  start,
  text,
  words: [],
  translation: null,
});

const plain = [line('One'), line('Two'), line(''), line('Three')];

describe('tap to sync', () => {
  it('times lines in order and skips verse breaks', () => {
    let state = startSync(plain);
    expect(state.cursor).toBe(0);
    state = tap(state, 1000);
    state = tap(state, 2500.4);
    expect(state.cursor).toBe(3);
    state = tap(state, 6000);
    expect(state.cursor).toBe(4);
    expect(state.lines.map((l) => l.start)).toEqual([1000, 2500, null, 6000]);
    // Past the end, taps do nothing.
    expect(tap(state, 7000)).toBe(state);
  });

  it('starts at the first line without a time', () => {
    expect(startSync([line('a', 0), line('b'), line('c')]).cursor).toBe(1);
    expect(startSync([line(''), line('a', 0)]).cursor).toBe(1);
  });

  it('undoes the last tap and rewinds a little before it', () => {
    let state = tap(tap(startSync(plain), 1000), 5000);
    const undone = undo(state);
    state = undone.state;
    expect(undone.seekTo).toBe(2000);
    expect(state.cursor).toBe(1);
    expect(state.lines[1]?.start).toBeNull();
    expect(undo(undo(state).state).seekTo).toBeNull();
  });

  it('moves word timings with a re-timed line', () => {
    const sung: LyricsLine = {
      ...line('Hello world', 1000),
      words: [
        { start: 1000, text: 'Hello ' },
        { start: 1600, text: 'world' },
      ],
    };
    const state = tap({ lines: [sung], cursor: 0, history: [] }, 1500);
    expect(state.lines[0]?.words.map((w) => w.start)).toEqual([1500, 2100]);
    // A line without a time had no usable word timings.
    expect(tap(startSync([{ ...sung, start: null }]), 0).lines[0]?.words).toEqual([]);
  });

  it('finds the next line to tap', () => {
    expect(nextLine(plain, 1)).toBe(3);
    expect(nextLine(plain, 3)).toBe(4);
  });
});

describe('shiftAll', () => {
  it('moves every time but not below zero', () => {
    const shifted = shiftAll([line('a', 100), line('b', 2000), line('c')], -500);
    expect(shifted.map((l) => l.start)).toEqual([0, 1500, null]);
  });
});

describe('syncedLyrics', () => {
  it('keeps timed lines and counts the ones left out', () => {
    const { lyrics, untimed } = syncedLyrics([line('a', 0), line(''), line('b')], []);
    expect(lyrics.synced).toBe(true);
    expect(lyrics.lines.map((l) => l.text)).toEqual(['a']);
    expect(untimed).toBe(1);
  });

  it('leaves plain lyrics plain', () => {
    const { lyrics, untimed } = syncedLyrics(plain, [['ar', 'X']]);
    expect(lyrics.synced).toBe(false);
    expect(lyrics.lines).toHaveLength(4);
    expect(lyrics.tags).toEqual([['ar', 'X']]);
    expect(untimed).toBe(0);
  });
});
