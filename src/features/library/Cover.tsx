import { User } from 'lucide-react';

import { ArtworkPlaceholder } from '../../components/CoverArt/index.ts';

import { coverUrl } from '../../lib/library.ts';
import styles from './Cover.module.css';

export interface CoverProps {
  id: string | null;
  /** Artists get a round picture and a person placeholder. */
  shape?: 'square' | 'round';
  className?: string;
}

/** A cached album cover in the theme's shape, or Vitra's symbol in its place. */
export function Cover({ id, shape = 'square', className }: CoverProps) {
  const classes = [styles.cover, shape === 'round' && styles.round, className]
    .filter(Boolean)
    .join(' ');
  if (id) {
    return (
      <img
        className={classes}
        src={coverUrl(id)}
        alt=""
        loading="lazy"
        decoding="async"
        draggable={false}
      />
    );
  }
  return (
    <div className={`${classes} ${styles.placeholder}`} aria-hidden="true">
      {shape === 'round' ? (
        <User className={styles.icon} />
      ) : (
        <ArtworkPlaceholder className={styles.symbol} />
      )}
    </div>
  );
}
