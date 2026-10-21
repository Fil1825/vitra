import { Disc3, User } from 'lucide-react';

import { coverUrl } from '../../lib/library.ts';
import styles from './Cover.module.css';

export interface CoverProps {
  id: string | null;
  /** Artists get a round picture and a person placeholder. */
  shape?: 'square' | 'round';
  className?: string;
}

/** A cached album cover, or a placeholder in theme colors. */
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
  const Icon = shape === 'round' ? User : Disc3;
  return (
    <div className={`${classes} ${styles.placeholder}`} aria-hidden="true">
      <Icon className={styles.icon} />
    </div>
  );
}
