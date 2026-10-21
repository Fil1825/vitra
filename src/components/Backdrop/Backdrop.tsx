import type { CSSProperties } from 'react';

import { backdropColors } from '../../theme/tokens.ts';
import styles from './Backdrop.module.css';

export interface BackdropProps {
  /** Colors to flow between; defaults to the theme gradient. */
  colors?: readonly string[];
  /** Small artwork image shown heavily blurred behind everything. */
  artworkUrl?: string;
}

/**
 * Window background: the theme base color over the system vibrancy, with a slowly flowing
 * gradient on top. Plain radial gradients (no `filter: blur`) keep it cheap to animate.
 */
export function Backdrop({ colors, artworkUrl }: BackdropProps) {
  const style = colors
    ? (Object.fromEntries(
        backdropColors(colors).map((color, i) => [`--vitra-backdrop-${i + 1}`, color]),
      ) as CSSProperties)
    : undefined;

  return (
    <div className={styles.backdrop} style={style} aria-hidden="true">
      <div className={styles.base} />
      {artworkUrl && (
        // Keyed by URL so a new cover fades in instead of popping.
        <div
          key={artworkUrl}
          className={styles.artwork}
          style={{ backgroundImage: `url("${artworkUrl}")` }}
        />
      )}
      <div className={styles.flow}>
        <span className={styles.blob} />
        <span className={styles.blob} />
        <span className={styles.blob} />
      </div>
    </div>
  );
}
