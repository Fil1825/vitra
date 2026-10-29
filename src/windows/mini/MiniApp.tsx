import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  EyeOff,
  LoaderCircle,
  Maximize2,
  MicVocal,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useState, type CSSProperties, type PointerEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { useAppearanceController } from '../../app/useAppearanceController.ts';
import { CoverArt } from '../../components/CoverArt/index.ts';
import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { Slider } from '../../components/ui/Slider.tsx';
import { lyricsClock, useFrames } from '../../features/lyrics/clock.ts';
import { setLyricsVisible } from '../../features/lyrics/actions.ts';
import { Karaoke } from '../../features/lyrics/Karaoke.tsx';
import { useCurrentLibraryTrack } from '../../features/library/useCurrentTrack.ts';
import { dislikeTrack } from '../../features/wave/actions.ts';
import { DislikeButton, LikeButton } from '../../features/wave/WaveButtons.tsx';
import { bus } from '../../lib/bus.ts';
import { formatTime } from '../../lib/format.ts';
import { library } from '../../lib/library.ts';
import { lineAt, lineEnd } from '../../lib/lyrics.ts';
import { showContextMenu } from '../../lib/menu.ts';
import { mini } from '../../lib/mini.ts';
import { artworkUrl, player } from '../../lib/player.ts';
import { hasNext } from '../../lib/queue.ts';
import { useShortcuts } from '../../shortcuts/useShortcuts.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useMediaQuery } from '../../lib/useMediaQuery.ts';
import { useLyrics } from '../../stores/lyrics.ts';
import { useLivePosition, usePlayer } from '../../stores/player.ts';
import { useQueue } from '../../stores/queue.ts';
import { useSettings } from '../../stores/settings.ts';
import { miniMenu } from './menu.ts';
import styles from './MiniApp.module.css';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Pressing on these doesn't drag the window. */
const CONTROLS = 'button, input, a, [role="slider"]';

/** Whether a click-through mini player takes clicks right now (⌥ / Alt held over it). */
function useInteractive(): boolean {
  const [interactive, setInteractive] = useState(false);
  useEffect(() => bus.on('mini:interactive', setInteractive), []);
  return interactive;
}

/**
 * The mini player window: one glass slab over the system backdrop (faded by the opacity
 * setting), the artwork's colors washed in, and the mode's content. Everything is sized in
 * `em` of a font that follows the window's height, so dragging it bigger scales it all.
 */
export function MiniApp() {
  useAppearanceController();
  useShortcuts('mini');
  const { t } = useTranslation();
  const allSettings = useSettings((s) => s.settings);
  const settings = allSettings.miniPlayer;
  const update = useSettings((s) => s.update);
  const interactive = useInteractive();
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const washed = useAppearance((s) => s.theme.background.useArtwork);
  const inWave = useQueue((s) => s.queue.wave !== null);
  const libraryTrack = useCurrentLibraryTrack();

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0 || (event.target as Element).closest(CONTROLS)) {
      return;
    }
    getCurrentWindow().startDragging().catch(report('Moving the window'));
  };

  const openMenu = () =>
    showContextMenu(
      miniMenu(allSettings, t, {
        change: (change) =>
          void update((current) => ({
            ...current,
            miniPlayer: { ...current.miniPlayer, ...change },
          })),
        showLyrics: setLyricsVisible,
        expand: () => void mini.expand().catch(report('Expanding')),
        close: () => void mini.hide().catch(report('Closing the mini player')),
        quit: () => void mini.quit().catch(report('Quitting')),
        wave:
          inWave && libraryTrack
            ? {
                favorite: libraryTrack.favorite,
                like: () =>
                  void library
                    .setFavorite(libraryTrack.id, !libraryTrack.favorite)
                    .catch(report('Favoriting')),
                dislike: () => dislikeTrack(libraryTrack.id),
              }
            : undefined,
      }),
    ).catch(report('The menu'));

  return (
    <Glass
      as="main"
      className={styles.mini}
      data-part="mini"
      data-mode={settings.mode}
      data-click-through={settings.clickThrough && !interactive ? '' : undefined}
      data-interactive={settings.clickThrough && interactive ? '' : undefined}
      aria-label={t('mini.title')}
      priority={10}
      style={{ '--mini-opacity': settings.opacity } as CSSProperties}
      onPointerDown={onPointerDown}
      onContextMenu={(event) => {
        event.preventDefault();
        void openMenu();
      }}
    >
      {washed && artwork && (
        <div
          className={styles.wash}
          style={{ backgroundImage: `url("${artworkUrl(artwork.id, 'thumb')}")` }}
          aria-hidden="true"
        />
      )}
      {settings.mode === 'compact' && <Compact />}
      {settings.mode === 'slim' && <Slim />}
      {settings.mode === 'lyrics' && <LyricsLine />}
    </Glass>
  );
}

/** Back to the main window, and closing: shown while the pointer is over the mini player. */
function WindowButtons() {
  const { t } = useTranslation();
  return (
    <div className={styles.windowButtons}>
      <IconButton
        className={styles.small}
        label={t('mini.expand')}
        onClick={() => mini.expand().catch(report('Expanding'))}
      >
        <Maximize2 size="0.85em" />
      </IconButton>
      <IconButton
        className={styles.small}
        label={t('mini.close')}
        onClick={() => mini.hide().catch(report('Closing the mini player'))}
      >
        <X size="0.95em" />
      </IconButton>
    </div>
  );
}

/** Previous, play/pause, next; in a wave (`wave`), the heart and "don't like" around them. */
function Transport({ wave = false }: { wave?: boolean }) {
  const { t } = useTranslation();
  const state = usePlayer((s) => s.state);
  const canNext = useQueue((s) => hasNext(s.queue));
  const inWave = useQueue((s) => s.queue.wave !== null);
  const libraryTrack = useCurrentLibraryTrack();
  const loading = state.track !== null && state.status === 'loading';
  const playing = state.status === 'playing' || loading;
  const waveTrack = wave && inWave ? libraryTrack : undefined;
  return (
    <div className={styles.transport}>
      {waveTrack && (
        <LikeButton
          trackId={waveTrack.id}
          favorite={waveTrack.favorite}
          className={styles.button}
          onClassName={styles.on}
          size="0.9em"
        />
      )}
      <IconButton
        className={styles.button}
        label={t('player.previous')}
        disabled={state.track === null}
        onClick={() => player.previous().catch(report('Previous'))}
      >
        <SkipBack size="1em" fill="currentColor" />
      </IconButton>
      <IconButton
        className={`${styles.button} ${styles.play}`}
        label={loading ? t('player.loading') : playing ? t('player.pause') : t('player.play')}
        disabled={state.track === null}
        aria-busy={loading || undefined}
        onClick={() => player.toggle().catch(report('Play/pause'))}
      >
        {loading ? (
          <LoaderCircle size="1.25em" className={styles.spin} />
        ) : playing ? (
          <Pause size="1.25em" fill="currentColor" data-morph="" />
        ) : (
          <Play size="1.25em" fill="currentColor" data-morph="" />
        )}
      </IconButton>
      <IconButton
        className={styles.button}
        label={t('player.next')}
        disabled={!canNext}
        onClick={() => player.next().catch(report('Next'))}
      >
        <SkipForward size="1em" fill="currentColor" />
      </IconButton>
      {waveTrack && <DislikeButton trackId={waveTrack.id} className={styles.button} size="0.9em" />}
    </div>
  );
}

function Artwork() {
  const artwork = usePlayer((s) => s.state.track?.artwork ?? null);
  const track = usePlayer((s) => s.state.track !== null);
  return (
    <CoverArt
      src={artwork ? artworkUrl(artwork.id) : null}
      live={track}
      className={styles.artwork}
    />
  );
}

function useTrackText() {
  const { t } = useTranslation();
  const track = usePlayer((s) => s.state.track);
  const subtitle = [track?.metadata.artist, track?.metadata.album].filter(Boolean).join(' — ');
  return { title: track?.title ?? t('player.notPlaying'), subtitle };
}

/** Narrower than this, the compact mode's progress bar goes under the artwork. */
const NARROW = '(max-aspect-ratio: 11 / 5)';

/**
 * Artwork, title, controls and progress: beside the artwork, or across the whole width under
 * it in a squarer window. Moved in the markup rather than by a grid, because WebKit spreads an
 * item spanning several grid rows over them.
 */
function Compact() {
  const { t } = useTranslation();
  const { title, subtitle } = useTrackText();
  const state = usePlayer((s) => s.state);
  const position = useLivePosition(250);
  const [scrub, setScrub] = useState<number | null>(null);
  const narrow = useMediaQuery(NARROW);
  const seekable =
    state.track !== null && (state.status === 'playing' || state.status === 'paused');
  const shown = scrub ?? position;

  const progress = (
    <div className={styles.progress}>
      <span className={styles.time}>{formatTime(shown)}</span>
      <Slider
        className={styles.slider}
        label={t('player.progress')}
        value={shown}
        max={state.duration ?? 0}
        step={0.1}
        disabled={!seekable || state.duration === null}
        onChange={setScrub}
        onCommit={(value) => {
          player
            .seek(value)
            .catch(report('Seeking'))
            .finally(() => setScrub(null));
        }}
      />
      <span className={styles.time}>
        {state.duration === null ? '' : `-${formatTime(Math.max(0, state.duration - shown))}`}
      </span>
    </div>
  );
  const text = (
    <div className={styles.text}>
      <span className={styles.title} title={title}>
        {title}
      </span>
      <span className={styles.subtitle} title={subtitle}>
        {subtitle}
      </span>
      <Transport wave />
      {!narrow && progress}
    </div>
  );

  return (
    <div className={styles.compact} data-narrow={narrow || undefined}>
      {narrow ? (
        <>
          <div className={styles.head}>
            <Artwork />
            {text}
          </div>
          {progress}
        </>
      ) : (
        <>
          <Artwork />
          {text}
        </>
      )}
      <WindowButtons />
    </div>
  );
}

/** One line: small artwork, title and artist, the main controls, progress along the edge. */
function Slim() {
  const { title, subtitle } = useTrackText();
  const position = useLivePosition(500);
  const duration = usePlayer((s) => s.state.duration);
  const fraction = duration ? Math.min(1, position / duration) : 0;
  return (
    <>
      <Artwork />
      <span className={styles.line} title={subtitle ? `${title} — ${subtitle}` : title}>
        <span className={styles.title}>{title}</span>
        {subtitle && <span className={styles.subtitle}> — {subtitle}</span>}
      </span>
      <Transport />
      <WindowButtons />
      <div
        className={styles.edge}
        style={{ transform: `scaleX(${fraction})` }}
        aria-hidden="true"
      />
    </>
  );
}

/** The playing line of the lyrics, karaoke style, and the one after it. */
function LyricsLine() {
  const { t } = useTranslation();
  const { title, subtitle } = useTrackText();
  const hasTrack = usePlayer((s) => s.state.track !== null);
  const current = useLyrics((s) => s.current);
  const showTranslation = useSettings((s) => s.settings.lyrics.showTranslation);
  const visible = useSettings((s) => s.settings.lyrics.visible);
  const { lyrics, offsetMs } = current;
  const synced = lyrics?.synced === true;
  const [index, setIndex] = useState<number | null>(null);

  useFrames(
    useCallback(
      () => setIndex(lyrics && synced ? lineAt(lyrics, lyricsClock(offsetMs)) : null),
      [lyrics, synced, offsetMs],
    ),
  );

  let body;
  if (!hasTrack) {
    body = <p className={styles.lyric}>{t('player.notPlaying')}</p>;
  } else if (!visible || !lyrics || !synced) {
    const status = !visible
      ? t('mini.lyricsHidden')
      : current.status === 'loading' || current.status === 'searching'
        ? t('mini.lyricsLoading')
        : lyrics
          ? t('mini.lyricsNotSynced')
          : t('mini.noLyrics');
    body = (
      <>
        <p className={styles.lyric}>{title}</p>
        <p className={styles.next}>{subtitle ? `${subtitle} · ${status}` : status}</p>
      </>
    );
  } else {
    const line = index === null ? null : lyrics.lines[index];
    const sung = line && line.text.trim() !== '' ? line : null;
    const upcoming = lyrics.lines.find((next, i) => i > (index ?? -1) && next.text.trim() !== '');
    const below = sung && showTranslation && sung.translation ? sung.translation : upcoming?.text;
    body = (
      <>
        {/* Keyed by line, so each new line rises in. */}
        <p key={index ?? 'start'} className={`${styles.lyric} ${styles.rise}`}>
          {sung === null ? (
            <span className={styles.pause}>♪</span>
          ) : sung.words.length > 0 && index !== null ? (
            <Karaoke line={sung} end={lineEnd(lyrics, index)} offsetMs={offsetMs} />
          ) : (
            sung.text
          )}
        </p>
        <p className={styles.next}>{below ?? ''}</p>
      </>
    );
  }

  return (
    <>
      <div className={styles.lyrics}>{body}</div>
      <div className={styles.lyricsControls}>
        <Transport />
        <IconButton
          className={`${styles.button} ${styles.lyricsToggle}`}
          label={t(visible ? 'mini.hideLyrics' : 'player.showLyrics')}
          onClick={() => setLyricsVisible(!visible)}
        >
          {visible ? <EyeOff size="0.95em" /> : <MicVocal size="0.95em" />}
        </IconButton>
        <WindowButtons />
      </div>
    </>
  );
}
