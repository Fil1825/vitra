import { Pause, Play, RotateCcw, Undo2 } from 'lucide-react';
import { useCallback, useEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { formatStamp, lineAt } from '../../lib/lyrics.ts';
import { player } from '../../lib/player.ts';
import { queue } from '../../lib/queue.ts';
import { livePosition, useLivePosition, usePlayer } from '../../stores/player.ts';
import type { LyricsEditorTarget } from '../../stores/ui.ts';
import styles from './LyricsEditor.module.css';
import { shiftAll, tap, undo, type SyncState } from './sync.ts';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

const SHIFTS = [-1000, -100, 100, 1000];

/** The playing position in milliseconds, as precise as the UI knows it. */
function nowMs(): number {
  const { state, positionAt } = usePlayer.getState();
  return livePosition(state, positionAt, performance.now()) * 1000;
}

function isTyping(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest('input, textarea, select') !== null)
  );
}

/**
 * Tap to sync: the track plays and Space times the highlighted line. Backspace takes the last
 * tap back, the arrows move the highlight, and the whole set can be shifted.
 */
export function SyncPane({
  target,
  state,
  onChange,
}: {
  target: LyricsEditorTarget;
  state: SyncState;
  onChange(state: SyncState): void;
}) {
  const { t } = useTranslation();
  const track = usePlayer((s) => s.state.track);
  const status = usePlayer((s) => s.state.status);
  const position = useLivePosition(50);
  const listRef = useRef<HTMLOListElement>(null);
  const playingThis = track?.path === target.path;
  const { lines, cursor } = state;

  const stateRef = useRef(state);
  const onChangeRef = useRef(onChange);
  useEffect(() => {
    stateRef.current = state;
    onChangeRef.current = onChange;
  });

  const takeBack = useCallback(() => {
    const { state: previous, seekTo } = undo(stateRef.current);
    onChangeRef.current(previous);
    if (seekTo !== null) {
      player.seek(seekTo / 1000).catch(report('Seeking'));
    }
  }, []);

  // Captured before the player's own shortcuts, so Space taps instead of pausing.
  useEffect(() => {
    if (!playingThis) {
      return;
    }
    const onKey = (event: KeyboardEvent) => {
      if (isTyping(event.target) || event.metaKey || event.ctrlKey || event.altKey) {
        return;
      }
      const current = stateRef.current;
      let handled = true;
      if (event.key === ' ') {
        onChangeRef.current(tap(current, nowMs()));
      } else if (event.key === 'Backspace') {
        takeBack();
      } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        const step = event.key === 'ArrowUp' ? -1 : 1;
        const next = Math.max(0, Math.min(current.lines.length, current.cursor + step));
        onChangeRef.current({ ...current, cursor: next });
      } else {
        handled = false;
      }
      if (handled) {
        event.preventDefault();
        event.stopPropagation();
      }
    };
    // A focused button would click on Space's release otherwise.
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key === ' ' && !isTyping(event.target)) {
        event.preventDefault();
      }
    };
    window.addEventListener('keydown', onKey, { capture: true });
    window.addEventListener('keyup', onKeyUp, { capture: true });
    return () => {
      window.removeEventListener('keydown', onKey, { capture: true });
      window.removeEventListener('keyup', onKeyUp, { capture: true });
    };
  }, [playingThis, takeBack]);

  useEffect(() => {
    listRef.current
      ?.querySelector(`[data-index="${cursor}"]`)
      ?.scrollIntoView({ block: 'nearest' });
  }, [cursor]);

  const play = () => {
    const opening =
      target.libraryId !== null ? queue.play([target.libraryId], 0) : player.open([target.path]);
    opening.catch(report('Playing the track'));
  };

  const playing = playingThis
    ? lineAt({ lines, synced: true, offsetMs: 0, tags: [] }, position * 1000)
    : null;

  return (
    <div className={styles.sync}>
      {playingThis ? (
        <div className={styles.transport}>
          <IconButton
            label={t('lyrics.editor.back5')}
            onClick={() => player.seek(Math.max(0, nowMs() / 1000 - 5)).catch(report('Seeking'))}
          >
            <RotateCcw size={15} />
          </IconButton>
          <IconButton
            label={status === 'playing' ? t('player.pause') : t('player.play')}
            onClick={() => player.toggle().catch(report('Playing'))}
          >
            {status === 'playing' ? (
              <Pause size={16} fill="currentColor" />
            ) : (
              <Play size={16} fill="currentColor" />
            )}
          </IconButton>
          <span className={styles.clock}>{formatStamp(position * 1000)}</span>
          <IconButton
            label={t('lyrics.editor.undo')}
            disabled={state.history.length === 0}
            onClick={takeBack}
          >
            <Undo2 size={15} />
          </IconButton>
          <span className={styles.shift} role="group" aria-label={t('lyrics.editor.shiftAll')}>
            <span className={styles.shiftLabel}>{t('lyrics.editor.shiftAll')}</span>
            {SHIFTS.map((delta) => (
              <button
                key={delta}
                type="button"
                className={styles.smallButton}
                onClick={() => onChange({ ...state, lines: shiftAll(lines, delta) })}
              >
                {delta > 0 ? '+' : '−'}
                {Math.abs(delta) / 1000} s
              </button>
            ))}
          </span>
        </div>
      ) : (
        <div className={styles.notPlaying}>
          <p>{t('lyrics.editor.playToSync')}</p>
          <button type="button" className={styles.primary} onClick={play}>
            <Play size={14} fill="currentColor" aria-hidden="true" />
            {t('lyrics.editor.playTrack')}
          </button>
        </div>
      )}
      <p className={styles.hint}>{t('lyrics.editor.syncHint')}</p>
      <ol ref={listRef} className={styles.syncList}>
        {lines.map((line, index) => (
          <li
            key={index}
            data-index={index}
            className={styles.syncLine}
            data-cursor={index === cursor || undefined}
            data-playing={index === playing || undefined}
          >
            <button
              type="button"
              className={styles.stamp}
              disabled={line.start === null || !playingThis}
              title={t('lyrics.editor.seekToLine')}
              onClick={() =>
                line.start !== null && player.seek(line.start / 1000).catch(report('Seeking'))
              }
            >
              {line.start === null ? '—' : formatStamp(line.start)}
            </button>
            <button
              type="button"
              className={styles.syncText}
              onClick={() => onChange({ ...state, cursor: index })}
            >
              {line.text || <span className={styles.break}>{t('lyrics.editor.break')}</span>}
            </button>
          </li>
        ))}
        <li
          data-index={lines.length}
          className={`${styles.syncLine} ${styles.end}`}
          data-cursor={cursor >= lines.length || undefined}
        >
          {t('lyrics.editor.end')}
        </li>
      </ol>
    </div>
  );
}
