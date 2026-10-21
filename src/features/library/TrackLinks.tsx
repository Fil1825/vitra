import type { ReactNode } from 'react';

import { useNavigation, type Route } from '../../stores/navigation.ts';
import styles from './TrackLinks.module.css';

/** Text that opens a library page when there is one to open, plain text otherwise. */
export function LibraryLink({
  to,
  className,
  children,
}: {
  to: Route | null;
  className?: string;
  children: ReactNode;
}) {
  const go = useNavigation((s) => s.go);
  if (to === null) {
    return <span className={className}>{children}</span>;
  }
  return (
    <button
      type="button"
      className={[styles.link, className].filter(Boolean).join(' ')}
      onClick={() => go(to)}
    >
      {children}
    </button>
  );
}
