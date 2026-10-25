import { useLayoutEffect, type RefObject } from 'react';

import { rememberScroll, restoreScroll, settleScroll } from './scrollMemory.ts';

/** How long a screen still loading may take to grow to its saved position. */
const SETTLE_MS = 2000;

/**
 * Keeps the scroll position of `ref` (marked `data-scroll-key={key}`) for the session and
 * puts it back when the screen comes again, before it is painted. A list that is still
 * loading gets there as it grows, unless the user scrolls first.
 */
export function useScrollMemory(ref: RefObject<HTMLElement | null>, key: string): void {
  useLayoutEffect(() => {
    const scroller = ref.current;
    if (!scroller) {
      return;
    }
    let done = restoreScroll(scroller);
    const observer = new ResizeObserver(() => {
      if (!done) done = restoreScroll(scroller);
      if (done) observer.disconnect();
    });
    const settle = () => {
      done = true;
      settleScroll(scroller);
      observer.disconnect();
    };
    if (!done) {
      for (const child of Array.from(scroller.children)) observer.observe(child);
    }
    const timer = window.setTimeout(settle, SETTLE_MS);
    // Our own scrolling while putting it back isn't the user's: only remember once settled.
    const onScroll = () => {
      if (done) rememberScroll(key, scroller.scrollTop);
    };
    const userEvents = ['wheel', 'touchstart', 'keydown', 'pointerdown'] as const;
    for (const type of userEvents) {
      scroller.addEventListener(type, settle, { passive: true });
    }
    scroller.addEventListener('scroll', onScroll, { passive: true });
    return () => {
      window.clearTimeout(timer);
      observer.disconnect();
      for (const type of userEvents) scroller.removeEventListener(type, settle);
      scroller.removeEventListener('scroll', onScroll);
    };
  }, [ref, key]);
}
