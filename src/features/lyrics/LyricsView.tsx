import {
  AArrowDown,
  AArrowUp,
  Languages,
  LoaderCircle,
  MicVocal,
  Minus,
  Pencil,
  Plus,
  Search,
} from 'lucide-react';
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import {
  formatOffset,
  lineAt,
  lineEnd,
  lyricsApi,
  wordProgress,
  type Lyrics,
  type LyricsLine,
} from '../../lib/lyrics.ts';
import { player } from '../../lib/player.ts';
import { FONT_SCALE_MAX, FONT_SCALE_MIN } from '../../lib/settings.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useLyrics } from '../../stores/lyrics.ts';
import { livePosition, usePlayer } from '../../stores/player.ts';
import { useSettings } from '../../stores/settings.ts';
import { openLyricsEditor } from './actions.ts';
import styles from './LyricsView.module.css';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** After scrolling by hand, the view leaves the lyrics where they are for this long. */
const BROWSE_MS = 3000;
const OFFSET_STEP = 100;
const SCALE_STEP = 0.1;

/** The playing position in lyrics time (milliseconds), read fresh on every call. */
function lyricsClock(offsetMs: number): number {
  const { state, positionAt } = usePlayer.getState();
  return livePosition(state, positionAt, performance.now()) * 1000 - offsetMs;
}

/** Calls `tick` every frame while playing, and on every player update while paused. */
function useFrames(tick: () => void): void {
  const playing = usePlayer((s) => s.state.status === 'playing');
  useEffect(() => {
    let frame = 0;
    const loop = () => {
      tick();
      frame = requestAnimationFrame(loop);
    };
    if (playing) {
      loop();
      return () => cancelAnimationFrame(frame);
    }
    tick();
    // Seeks while paused.
    return usePlayer.subscribe(tick);
  }, [playing, tick]);
}

/** The playing track's lyrics: synced ones follow playback, plain ones just scroll. */
export function LyricsView({
  variant,
  heading,
  className,
}: {
  variant: 'panel' | 'stage';
  /** Shown at the start of the toolbar row. */
  heading?: ReactNode;
  className?: string;
}) {
  const { t } = useTranslation();
  const track = usePlayer((s) => s.state.track);
  const current = useLyrics((s) => s.current);
  const settings = useSettings((s) => s.settings.lyrics);
  const update = useSettings((s) => s.update);

  const mine = track !== null && current.trackId === track.id;
  const status = mine ? current.status : 'loading';
  const lyrics = mine && status === 'ready' ? current.lyrics : null;
  const hasTranslation = lyrics?.lines.some((line) => line.translation) ?? false;

  const setScale = (delta: number) =>
    void update((s) => ({
      ...s,
      lyrics: {
        ...s.lyrics,
        fontScale: Math.min(
          FONT_SCALE_MAX,
          Math.max(FONT_SCALE_MIN, Math.round((s.lyrics.fontScale + delta) * 10) / 10),
        ),
      },
    }));
  const setOffset = (offsetMs: number) =>
    lyricsApi.setOffset(offsetMs).catch(report('Changing the lyrics offset'));

  const toolbar = (
    <div className={styles.toolbar}>
      {heading}
      {lyrics && (
        <>
          <IconButton
            label={t('lyrics.smaller')}
            disabled={settings.fontScale <= FONT_SCALE_MIN}
            onClick={() => setScale(-SCALE_STEP)}
          >
            <AArrowDown size={16} />
          </IconButton>
          <IconButton
            label={t('lyrics.larger')}
            disabled={settings.fontScale >= FONT_SCALE_MAX}
            onClick={() => setScale(SCALE_STEP)}
          >
            <AArrowUp size={16} />
          </IconButton>
        </>
      )}
      {lyrics?.synced && (
        <span className={styles.offset} role="group" aria-label={t('lyrics.offset')}>
          <IconButton
            label={t('lyrics.earlier')}
            onClick={() => setOffset(current.offsetMs - OFFSET_STEP)}
          >
            <Minus size={14} />
          </IconButton>
          <button
            type="button"
            className={styles.offsetValue}
            title={t('lyrics.resetOffset')}
            disabled={current.offsetMs === 0}
            onClick={() => setOffset(0)}
          >
            {formatOffset(current.offsetMs)}
          </button>
          <IconButton
            label={t('lyrics.later')}
            onClick={() => setOffset(current.offsetMs + OFFSET_STEP)}
          >
            <Plus size={14} />
          </IconButton>
        </span>
      )}
      {hasTranslation && (
        <IconButton
          label={t(settings.showTranslation ? 'lyrics.hideTranslation' : 'lyrics.showTranslation')}
          aria-pressed={settings.showTranslation}
          className={settings.showTranslation ? styles.on : undefined}
          onClick={() =>
            void update((s) => ({
              ...s,
              lyrics: { ...s.lyrics, showTranslation: !s.lyrics.showTranslation },
            }))
          }
        >
          <Languages size={16} />
        </IconButton>
      )}
      {track && (
        <IconButton label={t('lyrics.edit')} onClick={() => openLyricsEditor(track)}>
          <Pencil size={15} />
        </IconButton>
      )}
    </div>
  );

  let body;
  if (!track) {
    body = <p className={styles.message}>{t('lyrics.nothingPlaying')}</p>;
  } else if (status === 'searching') {
    body = (
      <p className={styles.message} role="status">
        <LoaderCircle size={16} className={styles.spin} aria-hidden="true" />
        {t('lyrics.searching')}
      </p>
    );
  } else if (status === 'missing') {
    body = (
      <div className={styles.missing}>
        <MicVocal size={28} aria-hidden="true" />
        <p className={styles.missingTitle}>{t('lyrics.missing')}</p>
        <p className={styles.missingBody}>
          {t(settings.online ? 'lyrics.missingOnline' : 'lyrics.missingOffline')}
        </p>
        {current.error && <p className={styles.error}>{t('lyrics.searchFailed')}</p>}
        <div className={styles.actions}>
          <button type="button" className={styles.primary} onClick={() => openLyricsEditor(track)}>
            <Pencil size={14} aria-hidden="true" />
            {t('lyrics.add')}
          </button>
          <button
            type="button"
            className={styles.secondary}
            onClick={() => lyricsApi.searchOnline().catch(report('Searching for lyrics'))}
          >
            <Search size={14} aria-hidden="true" />
            {t('lyrics.searchOnline')}
          </button>
        </div>
      </div>
    );
  } else if (lyrics) {
    body = lyrics.synced ? (
      <SyncedLyrics
        key={track.id}
        lyrics={lyrics}
        offsetMs={current.offsetMs}
        showTranslation={settings.showTranslation}
      />
    ) : (
      <PlainLyrics lyrics={lyrics} showTranslation={settings.showTranslation} />
    );
  } else {
    // Loading: usually a few milliseconds, not worth a spinner.
    body = null;
  }

  return (
    <div
      className={`${styles.view} ${className ?? ''}`}
      data-variant={variant}
      style={{ '--lyrics-scale': settings.fontScale } as CSSProperties}
    >
      {toolbar}
      {body}
      {lyrics && current.source === 'online' && (
        <p className={styles.credit}>{t('lyrics.fromLrclib')}</p>
      )}
    </div>
  );
}

function PlainLyrics({ lyrics, showTranslation }: { lyrics: Lyrics; showTranslation: boolean }) {
  return (
    <div className={styles.scroller}>
      <div className={styles.plain}>
        {lyrics.lines.map((line, index) => (
          <p key={index} className={styles.plainLine}>
            {line.text || '\u00a0'}
            {showTranslation && line.translation && (
              <span className={styles.translation}>{line.translation}</span>
            )}
          </p>
        ))}
      </div>
    </div>
  );
}

function SyncedLyrics({
  lyrics,
  offsetMs,
  showTranslation,
}: {
  lyrics: Lyrics;
  offsetMs: number;
  showTranslation: boolean;
}) {
  const motion = useAppearance((s) => s.effects.motion);
  const blur = useAppearance((s) => s.effects.blur && s.effects.motion);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<number | null>(null);
  const [browsing, setBrowsing] = useState(false);
  const browseTimer = useRef(0);
  const first = useRef(true);

  useFrames(
    useCallback(() => setActive(lineAt(lyrics, lyricsClock(offsetMs))), [lyrics, offsetMs]),
  );

  // Keeps the playing line a third of the way down.
  useLayoutEffect(() => {
    const scroller = scrollerRef.current;
    if (!scroller || browsing) {
      return;
    }
    const target =
      active === null ? null : scroller.querySelector<HTMLElement>(`[data-index="${active}"]`);
    const top = target ? target.offsetTop - scroller.clientHeight / 3 : 0;
    scroller.scrollTo({ top, behavior: motion && !first.current ? 'smooth' : 'auto' });
    first.current = false;
  }, [active, browsing, motion]);

  useEffect(() => () => window.clearTimeout(browseTimer.current), []);

  const browse = () => {
    setBrowsing(true);
    window.clearTimeout(browseTimer.current);
    browseTimer.current = window.setTimeout(() => setBrowsing(false), BROWSE_MS);
  };

  const seek = (line: LyricsLine) => {
    if (line.start === null) {
      return;
    }
    window.clearTimeout(browseTimer.current);
    setBrowsing(false);
    player.seek(Math.max(0, line.start + offsetMs) / 1000).catch(report('Seeking'));
  };

  const firstStart = lyrics.lines[0]?.start ?? 0;

  return (
    <div
      ref={scrollerRef}
      className={styles.scroller}
      data-browsing={browsing || undefined}
      data-blur={blur || undefined}
      onWheel={browse}
      onTouchMove={browse}
    >
      <div className={styles.lines}>
        <Gap active={active === null && firstStart > 0} />
        {lyrics.lines.map((line, index) => {
          if (line.text.trim() === '') {
            return <Gap key={index} index={index} active={index === active} />;
          }
          const distance = active === null ? index + 1 : Math.abs(index - active);
          return (
            <button
              key={index}
              type="button"
              data-index={index}
              className={styles.line}
              data-state={
                index === active ? 'active' : active !== null && index < active ? 'past' : 'future'
              }
              aria-current={index === active || undefined}
              style={{ '--distance': Math.min(distance, 4) } as CSSProperties}
              onClick={() => seek(line)}
            >
              {index === active && line.words.length > 0 ? (
                <Karaoke line={line} end={lineEnd(lyrics, index)} offsetMs={offsetMs} />
              ) : (
                <span className={styles.text}>{line.text}</span>
              )}
              {showTranslation && line.translation && (
                <span className={styles.translation}>{line.translation}</span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** An instrumental break: three dots that breathe while it lasts. */
function Gap({ index, active }: { index?: number; active: boolean }) {
  return (
    <div
      className={styles.gap}
      data-index={index}
      data-active={active || undefined}
      aria-hidden="true"
    >
      <span />
      <span />
      <span />
    </div>
  );
}

/** The playing line with word timings: sung words light up, the current one fills in. */
function Karaoke({ line, end, offsetMs }: { line: LyricsLine; end: number; offsetMs: number }) {
  const wordsRef = useRef<HTMLSpanElement>(null);

  // Updated outside React: a re-render per frame for one line isn't worth it.
  useFrames(
    useCallback(() => {
      const spans = wordsRef.current?.children;
      if (!spans) {
        return;
      }
      const { index, fraction } = wordProgress(line, end, lyricsClock(offsetMs));
      for (let i = 0; i < spans.length; i++) {
        const span = spans[i] as HTMLElement;
        const state = i < index ? 'sung' : i === index ? 'singing' : 'upcoming';
        if (span.dataset.state !== state) {
          span.dataset.state = state;
        }
        if (i === index) {
          // The soft edge of the fill runs from just before the word to just past it.
          span.style.setProperty('--fill', `${(fraction * 116 - 16).toFixed(1)}%`);
        }
      }
    }, [line, end, offsetMs]),
  );

  return (
    <span ref={wordsRef} className={styles.text}>
      {line.words.map((word, index) => (
        <span key={index} className={styles.word} data-state="upcoming">
          {word.text}
        </span>
      ))}
    </span>
  );
}
