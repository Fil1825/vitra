import { Fragment, type MouseEvent, type ReactNode } from 'react';

import type { ArtistRef } from '../../lib/library.ts';
import { useNavigation, type Route } from '../../stores/navigation.ts';
import styles from './TrackLinks.module.css';

/** Text that opens a library page when there is one to open, plain text otherwise. */
export function LibraryLink({
  to,
  className,
  inRow = false,
  children,
}: {
  to: Route | null;
  className?: string;
  /** Inside a selectable row: the click opens the page and doesn't select or play the row. */
  inRow?: boolean;
  children: ReactNode;
}) {
  const go = useNavigation((s) => s.go);
  if (to === null) {
    return <span className={className}>{children}</span>;
  }
  const stop = (event: MouseEvent) => inRow && event.stopPropagation();
  if (inRow) {
    // Plain text, unlike a button, so a long list of artists ends in an ellipsis. Rows are
    // reached by the keyboard as a whole, so the link needs no focus of its own.
    return (
      <span
        role="link"
        className={[styles.link, className].filter(Boolean).join(' ')}
        onClick={(event) => {
          stop(event);
          go(to);
        }}
        onDoubleClick={stop}
      >
        {children}
      </span>
    );
  }
  return (
    <button
      type="button"
      className={[styles.link, className].filter(Boolean).join(' ')}
      onClick={(event) => {
        stop(event);
        go(to);
      }}
      onDoubleClick={stop}
    >
      {children}
    </button>
  );
}

/** Each of a track's artists as its own link ("A, B"); `fallback` when the library doesn't
 * know them (a file outside it, a link not added). */
export function ArtistLinks({
  artists,
  fallback,
  inRow,
}: {
  artists: readonly ArtistRef[] | undefined;
  fallback?: string | null;
  inRow?: boolean;
}) {
  if (!artists || artists.length === 0) {
    return fallback ? <span>{fallback}</span> : null;
  }
  return (
    <>
      {artists.map((artist, index) => (
        <Fragment key={artist.id}>
          {index > 0 && ', '}
          <LibraryLink to={{ view: 'artist', id: artist.id }} inRow={inRow}>
            {artist.name}
          </LibraryLink>
        </Fragment>
      ))}
    </>
  );
}
