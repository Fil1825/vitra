import { useEffect, useRef, type CSSProperties } from 'react';

import { bus } from '../../lib/bus.ts';
import { backdropColors } from '../../theme/tokens.ts';
import styles from './Backdrop.module.css';

export interface BackdropProps {
  /** Colors to flow between; defaults to the theme gradient. */
  colors?: readonly string[];
  /** Small artwork image (already blurred) shown behind everything. */
  artworkUrl?: string;
}

/**
 * Window background: the theme base color over the system vibrancy, the artwork's colors
 * slowly flowing and breathing, and the theme gradient on top. Everything that moves only
 * changes `transform` or `opacity`, which the compositor animates without repainting, and
 * no moving layer has a `filter` (the artwork comes blurred from the backend).
 */
export function Backdrop({ colors, artworkUrl }: BackdropProps) {
  const root = useRef<HTMLDivElement>(null);

  // The music's pulse goes straight onto this element: on <html> it would restyle the whole
  // window a dozen times a second.
  useEffect(
    () =>
      bus.on('player:level', (level) =>
        root.current?.style.setProperty('--vitra-pulse', String(level)),
      ),
    [],
  );

  const style = colors
    ? (Object.fromEntries(
        backdropColors(colors).map((color, i) => [`--vitra-backdrop-${i + 1}`, color]),
      ) as CSSProperties)
    : undefined;

  return (
    <div ref={root} className={styles.backdrop} style={style} aria-hidden="true">
      <div className={styles.base} />
      <div className={styles.pulse}>
        {artworkUrl && (
          // Keyed by URL so a new cover fades in instead of popping.
          <div
            key={artworkUrl}
            className={styles.artwork}
            style={{ '--_artwork': `url("${artworkUrl}")` } as CSSProperties}
          >
            <span className={styles.piece} />
            <span className={styles.piece} />
            <span className={styles.piece} />
          </div>
        )}
        <div className={styles.flow}>
          <span className={styles.blob} />
          <span className={styles.blob} />
          <span className={styles.blob} />
        </div>
      </div>
      <div className={styles.glow} />
    </div>
  );
}
