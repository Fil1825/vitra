import { useLayoutEffect, useState, type RefObject } from 'react';

import { restoreScroll } from './scrollMemory.ts';

export interface Range {
  start: number;
  /** Exclusive. */
  end: number;
}

/**
 * Rows of a fixed-height list that intersect the viewport (plus `overscan` on each side).
 * `listTop` is where the list starts within the scrolled content.
 */
export function visibleRange(
  scrollTop: number,
  viewportHeight: number,
  listTop: number,
  count: number,
  rowHeight: number,
  overscan: number,
): Range {
  if (count === 0 || rowHeight <= 0) {
    return { start: 0, end: 0 };
  }
  const top = scrollTop - listTop;
  const first = Math.floor(top / rowHeight) - overscan;
  const last = Math.ceil((top + viewportHeight) / rowHeight) + overscan;
  const start = Math.min(Math.max(0, first), count);
  return { start, end: Math.min(count, Math.max(start, last)) };
}

/** The closest ancestor that scrolls (marked with `data-scroll-container`). */
export function scrollParent(element: HTMLElement): HTMLElement | null {
  return element.closest<HTMLElement>('[data-scroll-container]');
}

/** Scrolls the list's scroll container just enough to show row `index`. */
export function scrollRowIntoView(
  list: HTMLElement | null,
  index: number,
  rowHeight: number,
): void {
  const scroller = list && scrollParent(list);
  if (!list || !scroller) {
    return;
  }
  const listTop =
    list.getBoundingClientRect().top - scroller.getBoundingClientRect().top + scroller.scrollTop;
  const rowTop = listTop + index * rowHeight;
  if (rowTop < scroller.scrollTop) {
    scroller.scrollTop = rowTop;
  } else if (rowTop + rowHeight > scroller.scrollTop + scroller.clientHeight) {
    scroller.scrollTop = rowTop + rowHeight - scroller.clientHeight;
  }
}

/** Visible rows of a fixed-height list inside a `data-scroll-container`. */
export function useVirtualRange(
  listRef: RefObject<HTMLElement | null>,
  count: number,
  rowHeight: number,
  overscan = 8,
): Range {
  // Before the first measurement, render one screenful.
  const [range, setRange] = useState<Range>({ start: 0, end: Math.min(count, 40) });

  // Measured before paint, after the screen's saved position is back (its scroller
  // remembers it: see scrollMemory.ts), so a list shown again doesn't flash empty rows.
  useLayoutEffect(() => {
    const list = listRef.current;
    const scroller = list && scrollParent(list);
    if (!list || !scroller) {
      return;
    }
    restoreScroll(scroller);
    let frame = 0;
    const update = () => {
      frame = 0;
      const listTop =
        list.getBoundingClientRect().top -
        scroller.getBoundingClientRect().top +
        scroller.scrollTop;
      const next = visibleRange(
        scroller.scrollTop,
        scroller.clientHeight,
        listTop,
        count,
        rowHeight,
        overscan,
      );
      setRange((current) =>
        current.start === next.start && current.end === next.end ? current : next,
      );
    };
    const schedule = () => {
      if (frame === 0) {
        frame = requestAnimationFrame(update);
      }
    };
    update();
    scroller.addEventListener('scroll', schedule, { passive: true });
    const observer = new ResizeObserver(schedule);
    observer.observe(scroller);
    return () => {
      scroller.removeEventListener('scroll', schedule);
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [listRef, count, rowHeight, overscan]);

  return { start: Math.min(range.start, count), end: Math.min(range.end, count) };
}
