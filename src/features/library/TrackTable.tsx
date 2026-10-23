import { AudioLines, Globe, GripVertical, Heart, Play } from 'lucide-react';
import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { formatTime } from '../../lib/format.ts';
import type { LibraryTrack } from '../../lib/library.ts';
import { isRemotePath } from '../../lib/links.ts';
import { showContextMenu, type MenuEntry } from '../../lib/menu.ts';
import { useRowDrag } from '../../lib/reorder.ts';
import {
  EMPTY_SELECTION,
  select,
  selectForMenu,
  selectedRows,
  type Selection,
} from '../../lib/selection.ts';
import { scrollRowIntoView, useVirtualRange } from '../../lib/virtual.ts';
import { usePlayer } from '../../stores/player.ts';
import { playTracks, report, setFavorite, trackMenu } from './actions.ts';
import { Cover } from './Cover.tsx';
import { ArtistLinks } from './TrackLinks.tsx';
import { sortTracks, type Column, type Sort } from './sort.ts';
import styles from './TrackTable.module.css';
import { usePlaylists } from './usePlaylists.ts';

const ROW_HEIGHT = 40;

const WIDTHS: Record<Column, string> = {
  number: '44px',
  title: 'minmax(140px, 3fr)',
  artist: 'minmax(90px, 2fr)',
  album: 'minmax(90px, 2fr)',
  time: '56px',
  added: '92px',
  plays: '64px',
  favorite: '40px',
};

export interface TrackTableProps {
  tracks: LibraryTrack[];
  columns: Column[];
  /** `track` shows tag track numbers (albums), `index` the position (playlists). */
  numbering?: 'track' | 'index';
  /** Small covers next to titles, for lists that mix albums. */
  covers?: boolean;
  /** Column headers sort the list. */
  sortable?: boolean;
  /** Drag reordering; indices refer to `tracks`. */
  onMove?(from: number, to: number): void;
  /** Extra context menu entries for the selected rows (indices into `tracks`). */
  extraMenu?(rows: number[]): MenuEntry[];
}

/** A virtualized track list: select, play, reorder and open context menus. */
export function TrackTable({
  tracks,
  columns,
  numbering = 'track',
  covers = false,
  sortable = false,
  onMove,
  extraMenu,
}: TrackTableProps) {
  const { t, i18n } = useTranslation();
  const [sort, setSort] = useState<Sort | null>(null);
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const [focus, setFocus] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const allPlaylists = usePlaylists();
  const playingId = usePlayer((s) => s.state.track?.libraryId ?? null);

  // A new list (another album, a refetch after a change) starts unselected.
  const [shownTracks, setShownTracks] = useState(tracks);
  if (shownTracks !== tracks) {
    setShownTracks(tracks);
    setSelection(EMPTY_SELECTION);
  }

  const rows = useMemo(
    () => (sort ? sortTracks(tracks, sort, i18n.language) : tracks),
    [tracks, sort, i18n.language],
  );
  // Reordering only makes sense in the list's own order.
  const reorderable = onMove !== undefined && sort === null;
  const range = useVirtualRange(listRef, rows.length, ROW_HEIGHT);
  const drag = useRowDrag(listRef, ROW_HEIGHT, rows.length, (from, to) => onMove?.(from, to));
  const template = [reorderable && '28px', ...columns.map((c) => WIDTHS[c])]
    .filter(Boolean)
    .join(' ');

  const ids = () => rows.map((track) => track.id);
  const play = (index: number) => playTracks(ids(), index);

  const onRowClick = (index: number, event: MouseEvent) => {
    setFocus(index);
    setSelection((s) =>
      select(s, index, { toggle: event.metaKey || event.ctrlKey, extend: event.shiftKey }),
    );
  };

  const onContextMenu = (index: number, event: MouseEvent) => {
    event.preventDefault();
    const next = selectForMenu(selection, index);
    setSelection(next);
    setFocus(index);
    const picked = selectedRows(next);
    const chosen = picked.map((row) => rows[row]).filter((track) => track !== undefined);
    showContextMenu(trackMenu(chosen, allPlaylists, extraMenu?.(picked))).catch(
      report('Showing a menu'),
    );
  };

  // Keyboard focus follows the focused row, scrolling it into view first.
  const pendingFocus = useRef(false);
  const moveFocus = (index: number, extend: boolean) => {
    const target = Math.max(0, Math.min(rows.length - 1, index));
    setFocus(target);
    setSelection((s) => select(s, target, { toggle: false, extend }));
    scrollRowIntoView(listRef.current, target, ROW_HEIGHT);
    pendingFocus.current = true;
  };
  useEffect(() => {
    if (pendingFocus.current) {
      pendingFocus.current = false;
      listRef.current?.querySelector<HTMLElement>(`[data-index="${focus}"]`)?.focus();
    }
  });

  const onKeyDown = (event: KeyboardEvent) => {
    if (rows.length === 0) {
      return;
    }
    const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
    if (step !== undefined && event.altKey && reorderable) {
      event.preventDefault();
      const to = Math.max(0, Math.min(rows.length - 1, focus + step));
      if (to !== focus) {
        onMove?.(focus, to);
        moveFocus(to, false);
      }
    } else if (step !== undefined) {
      event.preventDefault();
      moveFocus(focus + step, event.shiftKey);
    } else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      moveFocus(event.key === 'Home' ? 0 : rows.length - 1, event.shiftKey);
    } else if (event.key === 'Enter') {
      event.preventDefault();
      play(focus);
    }
  };

  const header = (column: Column) => {
    const label = t(`library.columns.${column}`);
    if (column === 'number' || column === 'favorite') {
      return (
        <span key={column} className={styles.headerCell} aria-label={label}>
          {column === 'number' ? label : ''}
        </span>
      );
    }
    if (!sortable) {
      return (
        <span key={column} className={styles.headerCell}>
          {label}
        </span>
      );
    }
    const active = sort?.key === column;
    return (
      <button
        key={column}
        type="button"
        className={styles.headerCell}
        data-active={active || undefined}
        aria-sort={active ? (sort.descending ? 'descending' : 'ascending') : undefined}
        onClick={() =>
          setSort((current) =>
            current?.key !== column
              ? { key: column, descending: column === 'added' || column === 'plays' }
              : current.descending === (column === 'added' || column === 'plays')
                ? { key: column, descending: !current.descending }
                : null,
          )
        }
      >
        {label}
        {active && <span aria-hidden="true">{sort.descending ? ' ↓' : ' ↑'}</span>}
      </button>
    );
  };

  const cell = (column: Column, track: LibraryTrack, index: number) => {
    switch (column) {
      case 'number': {
        const number = numbering === 'index' ? index + 1 : track.trackNumber;
        return (
          <span key={column} className={styles.number}>
            {track.id === playingId ? (
              <AudioLines size={15} className={styles.playing} aria-label={t('queue.playing')} />
            ) : (
              <span className={styles.numberText}>{number ?? ''}</span>
            )}
            <button
              type="button"
              className={styles.rowPlay}
              aria-label={t('library.play')}
              tabIndex={-1}
              onClick={(event) => {
                event.stopPropagation();
                play(index);
              }}
            >
              <Play size={13} fill="currentColor" />
            </button>
          </span>
        );
      }
      case 'title':
        return (
          <span key={column} className={styles.titleCell}>
            {covers && <Cover id={track.cover} className={styles.thumb} />}
            <span className={styles.text}>{track.title}</span>
            {isRemotePath(track.path) && (
              <Globe
                size={12}
                className={styles.remote}
                role="img"
                aria-label={t('library.streamed')}
              >
                <title>{t('library.streamed')}</title>
              </Globe>
            )}
          </span>
        );
      case 'artist':
        return (
          <span key={column} className={`${styles.text} ${styles.secondary}`}>
            <ArtistLinks artists={track.artists} fallback={track.artist} inRow />
          </span>
        );
      case 'album':
        return (
          <span key={column} className={`${styles.text} ${styles.secondary}`}>
            {track.album ?? ''}
          </span>
        );
      case 'time':
        return (
          <span key={column} className={styles.numeric}>
            {track.duration === null ? '' : formatTime(track.duration)}
          </span>
        );
      case 'added':
        return (
          <span key={column} className={styles.numeric}>
            {new Date(track.addedAt * 1000).toLocaleDateString(i18n.language)}
          </span>
        );
      case 'plays':
        return (
          <span key={column} className={styles.numeric}>
            {track.playCount || ''}
          </span>
        );
      case 'favorite':
        return (
          <span key={column} className={styles.favoriteCell}>
            <button
              type="button"
              className={styles.favorite}
              data-on={track.favorite || undefined}
              aria-pressed={track.favorite}
              aria-label={t(track.favorite ? 'player.unfavorite' : 'player.favorite')}
              tabIndex={-1}
              onClick={(event) => {
                event.stopPropagation();
                setFavorite([track.id], !track.favorite);
              }}
            >
              <Heart size={14} fill={track.favorite ? 'currentColor' : 'none'} />
            </button>
          </span>
        );
    }
  };

  const visible = rows.slice(range.start, range.end);
  const dropLine =
    drag.dragging &&
    drag.dragging.gap !== drag.dragging.from &&
    drag.dragging.gap !== drag.dragging.from + 1
      ? drag.dragging.gap * ROW_HEIGHT
      : null;

  return (
    <div className={styles.table}>
      <div className={styles.header} style={{ gridTemplateColumns: template }}>
        {reorderable && <span />}
        {columns.map(header)}
      </div>
      <div
        ref={listRef}
        className={styles.rows}
        role="listbox"
        aria-multiselectable="true"
        aria-label={t('library.songsSection')}
        style={{ height: rows.length * ROW_HEIGHT }}
        onKeyDown={onKeyDown}
        data-dragging={drag.dragging ? '' : undefined}
      >
        {visible.map((track, offset) => {
          const index = range.start + offset;
          return (
            <div
              key={`${track.id}-${index}`}
              role="option"
              aria-selected={selection.rows.has(index)}
              data-index={index}
              data-alt={index % 2 === 1 || undefined}
              data-playing={track.id === playingId || undefined}
              data-lifted={drag.dragging?.from === index || undefined}
              tabIndex={index === focus ? 0 : -1}
              className={styles.row}
              style={{ top: index * ROW_HEIGHT, gridTemplateColumns: template }}
              onClick={(event) => onRowClick(index, event)}
              onDoubleClick={() => play(index)}
              onContextMenu={(event) => onContextMenu(index, event)}
            >
              {reorderable && (
                <span
                  className={styles.grip}
                  aria-label={t('queue.reorder')}
                  title={t('queue.reorder')}
                  onPointerDown={(event) => drag.start(index, event)}
                >
                  <GripVertical size={14} />
                </span>
              )}
              {columns.map((column) => cell(column, track, index))}
            </div>
          );
        })}
        {dropLine !== null && <div className={styles.dropLine} style={{ top: dropLine }} />}
      </div>
    </div>
  );
}
