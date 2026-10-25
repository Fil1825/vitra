import { beforeEach, describe, expect, it } from 'vitest';

import {
  clampScroll,
  forgetScrolls,
  rememberScroll,
  restoreScroll,
  routeKey,
  savedScroll,
  settleScroll,
  type Scroller,
} from './scrollMemory.ts';

/** A scroll container of `content` px in a `height` px window. */
function scroller(key: string | undefined, content: number, height = 600): Scroller {
  const box = {
    top: 0,
    content,
    get scrollTop() {
      return box.top;
    },
    set scrollTop(value: number) {
      box.top = clampScroll(value, box.content, height);
    },
    get scrollHeight() {
      return box.content;
    },
    clientHeight: height,
    dataset: key === undefined ? {} : { scrollKey: key },
  };
  return box;
}

beforeEach(forgetScrolls);

describe('scroll memory', () => {
  it('keys each screen by its view and what it shows', () => {
    expect(routeKey({ view: 'playlist', id: 3 })).toBe('playlist:3');
    expect(routeKey({ view: 'album', id: 3 })).toBe('album:3');
    expect(routeKey({ view: 'songs' })).toBe('songs');
    expect(routeKey({ view: 'search', query: 'кино' })).toBe('search:кино');
  });

  it('puts a screen back where it was left, once', () => {
    rememberScroll('playlist:3', 2400);
    const list = scroller('playlist:3', 10_000);
    expect(restoreScroll(list)).toBe(true);
    expect(list.scrollTop).toBe(2400);
    // The user scrolls on; a later call (a virtual list measuring) changes nothing.
    list.scrollTop = 100;
    expect(restoreScroll(list)).toBe(true);
    expect(list.scrollTop).toBe(100);
  });

  it('keeps screens apart', () => {
    rememberScroll('playlist:3', 2400);
    rememberScroll('playlist:4', 800);
    const other = scroller('playlist:4', 10_000);
    restoreScroll(other);
    expect(other.scrollTop).toBe(800);
    const fresh = scroller('album:9', 10_000);
    restoreScroll(fresh);
    expect(fresh.scrollTop).toBe(0);
    expect(savedScroll('playlist:3')).toBe(2400);
  });

  it('stops at the end of a list that got shorter', () => {
    rememberScroll('songs', 9_000);
    const list = scroller('songs', 4_000);
    // Not all of it fits: a list still loading may grow to it.
    expect(restoreScroll(list)).toBe(false);
    expect(list.scrollTop).toBe(3_400);
    // It grew: the rest of the way.
    (list as unknown as { content: number }).content = 12_000;
    expect(restoreScroll(list)).toBe(true);
    expect(list.scrollTop).toBe(9_000);
  });

  it('gives up putting back once the user scrolls', () => {
    rememberScroll('songs', 9_000);
    const list = scroller('songs', 4_000);
    restoreScroll(list);
    settleScroll(list);
    (list as unknown as { content: number }).content = 12_000;
    list.scrollTop = 50;
    expect(restoreScroll(list)).toBe(true);
    expect(list.scrollTop).toBe(50);
  });

  it('leaves unkeyed scrollers alone', () => {
    rememberScroll('songs', 900);
    const queue = scroller(undefined, 4_000);
    expect(restoreScroll(queue)).toBe(true);
    expect(queue.scrollTop).toBe(0);
  });

  it('forgets a screen scrolled back to the top', () => {
    rememberScroll('songs', 900);
    rememberScroll('songs', 0);
    expect(savedScroll('songs')).toBe(0);
  });
});
