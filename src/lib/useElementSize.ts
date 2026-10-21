import { useEffect, useState, type RefObject } from 'react';

export interface Size {
  width: number;
  height: number;
}

/** Border-box size of an element, updated on resize. `enabled` avoids observers nobody needs. */
export function useElementSize(ref: RefObject<HTMLElement | null>, enabled = true): Size | null {
  const [size, setSize] = useState<Size | null>(null);

  useEffect(() => {
    const element = ref.current;
    if (!enabled || !element) {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      const box = entry?.borderBoxSize[0];
      if (box) {
        setSize({ width: Math.round(box.inlineSize), height: Math.round(box.blockSize) });
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [ref, enabled]);

  return enabled ? size : null;
}
