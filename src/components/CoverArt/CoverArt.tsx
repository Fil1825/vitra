import { useEffect, useRef } from 'react';

import { useAppearance } from '../../stores/appearance.ts';
import styles from './CoverArt.module.css';
import { ArtworkPlaceholder } from './Placeholder.tsx';
import { spin } from './spin.ts';
import { useVinyl } from './useVinyl.ts';

export interface CoverArtProps {
  /** The picture, or `null` for the placeholder. */
  src: string | null;
  /** The playing track's cover: as a record, it turns while the music plays. */
  live?: boolean;
  /** Sizes it; `--cover-radius` there sets its corners while covers are square. */
  className?: string;
}

/**
 * A cover in the theme's shape (`--vitra-artwork-radius`). With vinyl on it is a record:
 * round, with grooves, a sheen that stays where the light is, and a spindle hole; the
 * playing track's covers turn (not with Reduce Motion).
 */
export function CoverArt({ src, live = false, className }: CoverArtProps) {
  const vinyl = useVinyl();
  const motion = useAppearance((s) => s.effects.motion);
  const disc = useRef<HTMLSpanElement>(null);
  const turning = vinyl && live && motion;

  useEffect(() => {
    const element = disc.current;
    if (!turning || !element) return;
    return spin(element);
  }, [turning]);

  return (
    <span className={`${styles.cover} ${className ?? ''}`} data-vinyl={vinyl || undefined}>
      <span ref={disc} className={styles.disc} data-turning={turning || undefined}>
        {src ? (
          <img className={styles.image} src={src} alt="" draggable={false} />
        ) : (
          <ArtworkPlaceholder className={styles.placeholder} />
        )}
        {vinyl && <span className={styles.grooves} />}
      </span>
      {vinyl && (
        <>
          <span className={styles.sheen} />
          <span className={styles.spindle} />
        </>
      )}
    </span>
  );
}
