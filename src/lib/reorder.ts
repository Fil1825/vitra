import { useCallback, useEffect, useRef, useState, type PointerEvent, type RefObject } from 'react';

import { scrollParent } from './virtual.ts';

/** The gap (0…count) a pointer at `y` (relative to the list's top) drops into. */
export function gapAt(y: number, rowHeight: number, count: number): number {
  return Math.min(count, Math.max(0, Math.round(y / rowHeight)));
}

/**
 * The index to move an item to so it lands in `gap`: after removing it from `from`, every
 * gap below it shifts up by one. `null` when the item wouldn't move.
 */
export function moveTarget(from: number, gap: number): number | null {
  const to = gap > from ? gap - 1 : gap;
  return to === from ? null : to;
}

/** Near the scroller's edges, dragging scrolls by up to this many pixels per frame. */
const EDGE = 48;
const MAX_SCROLL = 14;

export interface RowDrag {
  /** The row being dragged, and the gap it would drop into. */
  dragging: { from: number; gap: number } | null;
  /** Pointer-down handler for a row's drag handle. */
  start(index: number, event: PointerEvent<HTMLElement>): void;
}

/**
 * Drag-to-reorder for fixed-height rows (works with virtualized lists, unlike HTML5 drag
 * and drop, which the window's file-drop handling also intercepts).
 */
export function useRowDrag(
  listRef: RefObject<HTMLElement | null>,
  rowHeight: number,
  count: number,
  onMove: (from: number, to: number) => void,
): RowDrag {
  const [dragging, setDragging] = useState<RowDrag['dragging']>(null);
  const pointer = useRef<{ id: number; y: number } | null>(null);
  const onMoveRef = useRef(onMove);
  useEffect(() => {
    onMoveRef.current = onMove;
  });

  useEffect(() => {
    if (!dragging) {
      return;
    }
    const list = listRef.current;
    const scroller = list && scrollParent(list);
    if (!list || !scroller) {
      return;
    }
    const from = dragging.from;
    const updateGap = () => {
      if (pointer.current) {
        const top = list.getBoundingClientRect().top;
        const gap = gapAt(pointer.current.y - top, rowHeight, count);
        setDragging((d) => (d && d.gap !== gap ? { ...d, gap } : d));
      }
    };
    let frame = requestAnimationFrame(function scroll() {
      const y = pointer.current?.y;
      if (y !== undefined) {
        const bounds = scroller.getBoundingClientRect();
        const speed =
          y < bounds.top + EDGE
            ? -MAX_SCROLL * (1 - (y - bounds.top) / EDGE)
            : y > bounds.bottom - EDGE
              ? MAX_SCROLL * (1 - (bounds.bottom - y) / EDGE)
              : 0;
        if (speed !== 0) {
          scroller.scrollTop += Math.max(-MAX_SCROLL, Math.min(MAX_SCROLL, speed));
          updateGap();
        }
      }
      frame = requestAnimationFrame(scroll);
    });
    const move = (event: globalThis.PointerEvent) => {
      if (pointer.current?.id === event.pointerId) {
        pointer.current = { id: event.pointerId, y: event.clientY };
        updateGap();
      }
    };
    const end = (event: globalThis.PointerEvent) => {
      if (pointer.current?.id !== event.pointerId) {
        return;
      }
      pointer.current = null;
      setDragging((d) => {
        const to = d && event.type === 'pointerup' ? moveTarget(from, d.gap) : null;
        if (to !== null) {
          onMoveRef.current(from, to);
        }
        return null;
      });
    };
    const cancel = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        pointer.current = null;
        setDragging(null);
      }
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', end);
    window.addEventListener('pointercancel', end);
    window.addEventListener('keydown', cancel);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', end);
      window.removeEventListener('pointercancel', end);
      window.removeEventListener('keydown', cancel);
    };
    // Restart only when a drag starts or ends, not on every gap change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [dragging?.from, listRef, rowHeight, count]);

  const start = useCallback((index: number, event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    pointer.current = { id: event.pointerId, y: event.clientY };
    setDragging({ from: index, gap: index });
  }, []);

  return { dragging, start };
}
