import type { CSSProperties } from 'react';

import { backdropColors } from '../../theme/tokens.ts';
import styles from './Backdrop.module.css';

export interface BackdropProps {
  /** Colors to flow between; defaults to the theme gradient. Stage 2 passes artwork colors. */
  colors?: readonly string[];
}

/**
 * Window background: the theme base color over the system vibrancy, with a slowly flowing
 * gradient on top. Plain radial gradients (no `filter: blur`) keep it cheap to animate.
 */
export function Backdrop({ colors }: BackdropProps) {
  const style = colors
    ? (Object.fromEntries(
        backdropColors(colors).map((color, i) => [`--vitra-backdrop-${i + 1}`, color]),
      ) as CSSProperties)
    : undefined;

  return (
    <div className={styles.backdrop} style={style} aria-hidden="true">
      <div className={styles.base} />
      <div className={styles.flow}>
        <span className={styles.blob} />
        <span className={styles.blob} />
        <span className={styles.blob} />
      </div>
    </div>
  );
}
