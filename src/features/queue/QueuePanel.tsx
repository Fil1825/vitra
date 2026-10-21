import { AudioLines, GripVertical, X } from 'lucide-react';
import { useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { formatTime } from '../../lib/format.ts';
import { library } from '../../lib/library.ts';
import { queue, type QueueItem } from '../../lib/queue.ts';
import { useRowDrag } from '../../lib/reorder.ts';
import { useLibraryData } from '../../lib/useLibraryData.ts';
import { useVirtualRange } from '../../lib/virtual.ts';
import { useQueue } from '../../stores/queue.ts';
import { playTracks, report } from '../library/actions.ts';
import { Cover } from '../library/Cover.tsx';
import styles from './QueuePanel.module.css';

const ROW_HEIGHT = 52;
const HISTORY_LIMIT = 100;

type Tab = 'upNext' | 'history';

export function QueuePanel({ className }: { className?: string }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>('upNext');
  return (
    <Glass as="aside" className={`${styles.panel} ${className ?? ''}`} priority={2}>
      <header className={styles.header}>
        <h2 className={styles.title}>{t('queue.title')}</h2>
        <SegmentedControl<Tab>
          label={t('queue.title')}
          value={tab}
          options={[
            { value: 'upNext', label: t('queue.upNext') },
            { value: 'history', label: t('queue.history') },
          ]}
          onChange={setTab}
        />
      </header>
      {/* Its own scroller: the virtualized lists measure against it. */}
      <div className={styles.scroll} data-scroll-container>
        {tab === 'upNext' ? <UpNext /> : <History />}
      </div>
    </Glass>
  );
}

function Row({ item, playing }: { item: QueueItem; playing?: boolean }) {
  const { t } = useTranslation();
  return (
    <>
      <span className={styles.art}>
        <Cover id={item.cover} />
        {playing && (
          <span className={styles.playingBadge}>
            <AudioLines size={14} aria-label={t('queue.playing')} />
          </span>
        )}
      </span>
      <span className={styles.text}>
        <span className={styles.itemTitle}>{item.title}</span>
        <span className={styles.itemArtist}>{item.artist ?? ''}</span>
      </span>
      <span className={styles.time}>{item.duration === null ? '' : formatTime(item.duration)}</span>
    </>
  );
}

function UpNext() {
  const { t } = useTranslation();
  const state = useQueue((s) => s.queue);
  const listRef = useRef<HTMLDivElement>(null);
  const current = state.current;
  const currentItem = current === null ? undefined : state.items[current];
  // Upcoming items; their queue index is `offset + i`.
  const offset = current === null ? 0 : current + 1;
  const upcoming = state.items.slice(offset);
  const range = useVirtualRange(listRef, upcoming.length, ROW_HEIGHT);
  const drag = useRowDrag(listRef, ROW_HEIGHT, upcoming.length, (from, to) => {
    queue.move(offset + from, offset + to).catch(report('Reordering the queue'));
  });
  const [focus, setFocus] = useState(0);

  if (state.items.length === 0) {
    return (
      <div className={styles.empty}>
        <p className={styles.emptyTitle}>{t('queue.empty')}</p>
        <p>{t('queue.emptyBody')}</p>
      </div>
    );
  }

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    const step = { ArrowDown: 1, ArrowUp: -1 }[event.key];
    if (step !== undefined) {
      event.preventDefault();
      const target = Math.max(0, Math.min(upcoming.length - 1, index + step));
      if (event.altKey && target !== index) {
        queue.move(offset + index, offset + target).catch(report('Reordering the queue'));
      }
      setFocus(target);
      requestAnimationFrame(() =>
        listRef.current?.querySelector<HTMLElement>(`[data-index="${target}"]`)?.focus(),
      );
    } else if (event.key === 'Enter') {
      const item = upcoming[index];
      if (item) {
        queue.jump(item.uid).catch(report('Jumping in the queue'));
      }
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      const item = upcoming[index];
      if (item) {
        queue.remove([item.uid]).catch(report('Removing from the queue'));
      }
    }
  };

  const dropLine =
    drag.dragging &&
    drag.dragging.gap !== drag.dragging.from &&
    drag.dragging.gap !== drag.dragging.from + 1
      ? drag.dragging.gap * ROW_HEIGHT
      : null;

  return (
    <>
      {currentItem && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>{t('queue.playing')}</h3>
          <div className={`${styles.row} ${styles.current}`}>
            <Row item={currentItem} playing />
          </div>
        </section>
      )}
      <section className={styles.section}>
        <div className={styles.sectionHeader}>
          <h3 className={styles.sectionTitle}>{t('queue.upNext')}</h3>
          {upcoming.length > 0 && (
            <button
              type="button"
              className={styles.clear}
              onClick={() => queue.clear().catch(report('Clearing the queue'))}
            >
              {t('queue.clear')}
            </button>
          )}
        </div>
        <div
          ref={listRef}
          className={styles.list}
          role="list"
          aria-label={t('queue.upNext')}
          title={upcoming.length > 1 ? t('queue.moveHint') : undefined}
          style={{ height: upcoming.length * ROW_HEIGHT }}
          data-dragging={drag.dragging ? '' : undefined}
        >
          {upcoming.slice(range.start, range.end).map((item, i) => {
            const index = range.start + i;
            return (
              <div
                key={item.uid}
                role="listitem"
                className={styles.row}
                style={{ top: index * ROW_HEIGHT }}
                data-index={index}
                data-lifted={drag.dragging?.from === index || undefined}
                tabIndex={index === focus ? 0 : -1}
                onDoubleClick={() => queue.jump(item.uid).catch(report('Jumping in the queue'))}
                onKeyDown={(event) => onKeyDown(event, index)}
              >
                <span
                  className={styles.grip}
                  title={t('queue.reorder')}
                  aria-hidden="true"
                  onPointerDown={(event) => drag.start(index, event)}
                >
                  <GripVertical size={14} />
                </span>
                <Row item={item} />
                <IconButton
                  label={t('queue.remove')}
                  className={styles.remove}
                  tabIndex={-1}
                  onClick={() => queue.remove([item.uid]).catch(report('Removing from the queue'))}
                >
                  <X size={14} />
                </IconButton>
              </div>
            );
          })}
          {dropLine !== null && <div className={styles.dropLine} style={{ top: dropLine }} />}
        </div>
      </section>
    </>
  );
}

function History() {
  const { t } = useTranslation();
  const { data } = useLibraryData('history', () => library.history(HISTORY_LIMIT));
  if (data?.length === 0) {
    return (
      <div className={styles.empty}>
        <p className={styles.emptyTitle}>{t('queue.emptyHistory')}</p>
      </div>
    );
  }
  return (
    <ul className={styles.history}>
      {(data ?? []).map(({ track, playedAt }) => (
        <li key={`${track.id}-${playedAt}`}>
          <button
            type="button"
            className={styles.historyRow}
            title={new Date(playedAt * 1000).toLocaleString()}
            onDoubleClick={() => playTracks([track.id])}
          >
            <Row
              item={{
                uid: 0,
                path: track.path,
                trackId: track.id,
                title: track.title,
                artist: track.artist,
                album: track.album,
                duration: track.duration,
                cover: track.cover,
              }}
            />
          </button>
        </li>
      ))}
    </ul>
  );
}
