import {
  Download,
  Link2,
  ListMusic,
  ListPlus,
  ListStart,
  LoaderCircle,
  Play,
  Radio,
  X,
} from 'lucide-react';
import { motion } from 'motion/react';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { formatTime } from '../../lib/format.ts';
import { links, type LinkPreview, type PlayMode, type PreviewTrack } from '../../lib/links.ts';
import { artworkUrl } from '../../lib/player.ts';
import { useNavigation } from '../../stores/navigation.ts';
import { linkError } from './errors.ts';
import styles from './LinkDialog.module.css';

type Lookup =
  | { state: 'idle' }
  | { state: 'resolving'; input: string }
  | { state: 'resolved'; preview: LinkPreview }
  | { state: 'failed'; message: string };

function Thumbnail({ id, live }: { id: string | null; live?: boolean }) {
  const Icon = live ? Radio : Link2;
  return (
    <div className={styles.thumbnail}>
      {id ? <img src={artworkUrl(id)} alt="" draggable={false} /> : <Icon aria-hidden="true" />}
    </div>
  );
}

function TrackSummary({ track }: { track: PreviewTrack }) {
  const { t } = useTranslation();
  const details = [track.artist, track.album].filter(Boolean).join(' · ');
  return (
    <div className={styles.summary}>
      <Thumbnail id={track.artwork} live={track.live} />
      <div className={styles.summaryText}>
        <h3 className={styles.name}>{track.title}</h3>
        {details && <p className={styles.details}>{details}</p>}
        <p className={styles.meta}>
          {track.live
            ? t('links.live')
            : track.duration !== null
              ? formatTime(track.duration)
              : null}
          {track.downloaded && <span className={styles.badge}>{t('links.downloaded')}</span>}
          {!track.downloaded && track.trackId !== null && (
            <span className={styles.badge}>{t('links.inLibrary')}</span>
          )}
        </p>
      </div>
    </div>
  );
}

/** Opens a pasted link: shows what it is, then plays, queues, imports or downloads it. */
export function LinkDialog({ initial, onClose }: { initial: string; onClose(): void }) {
  const { t } = useTranslation();
  const [input, setInput] = useState(initial);
  const [lookup, setLookup] = useState<Lookup>(() =>
    initial.trim() ? { state: 'resolving', input: initial.trim() } : { state: 'idle' },
  );
  const [busy, setBusy] = useState(false);
  const field = useRef<HTMLInputElement>(null);
  // Answers to an older lookup are dropped.
  const generation = useRef(0);

  /** Asks the backend about `value`; the lookup is already shown as resolving. */
  const fetchPreview = useCallback(
    (value: string) => {
      const current = ++generation.current;
      links.resolve(value).then(
        (preview) => {
          if (generation.current === current) setLookup({ state: 'resolved', preview });
        },
        (err: unknown) => {
          if (generation.current === current) {
            setLookup({ state: 'failed', message: linkError(err, t) });
          }
        },
      );
    },
    [t],
  );

  const resolve = (text: string) => {
    const value = text.trim();
    if (value) {
      setLookup({ state: 'resolving', input: value });
      fetchPreview(value);
    }
  };

  useEffect(() => {
    if (initial.trim()) {
      fetchPreview(initial.trim());
    } else {
      field.current?.focus();
    }
    // Only the link the dialog was opened with is looked up by itself.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
      onClose();
    } catch (err) {
      setLookup({ state: 'failed', message: linkError(err, t) });
      setBusy(false);
    }
  };

  const submit = (event: FormEvent) => {
    event.preventDefault();
    resolve(input);
  };

  const preview = lookup.state === 'resolved' ? lookup.preview : null;
  const url = preview?.kind === 'track' ? preview.track.url : preview?.url;
  const play = (mode: PlayMode) => () => url && void run(() => links.play(url, mode));
  const download = () =>
    url &&
    void run(async () => {
      await links.download(url);
      useNavigation.getState().go({ view: 'downloads' });
    });
  const importPlaylist = () =>
    url &&
    void run(async () => {
      const playlist = await links.import(url);
      useNavigation.getState().go({ view: 'playlist', id: playlist.id });
    });

  return (
    <div className={styles.backdrop} onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      {/* Only transform animates: opacity on an ancestor would cut the glass off its backdrop. */}
      <motion.div
        initial={{ y: 24, scale: 0.98 }}
        animate={{ y: 0, scale: 1 }}
        transition={{ type: 'spring', stiffness: 420, damping: 34 }}
      >
        <Glass
          as="section"
          className={styles.dialog}
          role="dialog"
          aria-modal="true"
          aria-labelledby="link-dialog-title"
          priority={7}
        >
          <header className={styles.header}>
            <h2 id="link-dialog-title" className={styles.title}>
              {t('links.title')}
            </h2>
            <IconButton label={t('links.close')} onClick={onClose}>
              <X size={18} />
            </IconButton>
          </header>

          <form className={styles.form} onSubmit={submit}>
            <input
              ref={field}
              className={styles.input}
              type="url"
              value={input}
              placeholder={t('links.placeholder')}
              aria-label={t('links.address')}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setInput(event.target.value)}
            />
            <button
              type="submit"
              className={styles.secondary}
              disabled={!input.trim() || lookup.state === 'resolving'}
            >
              {t('links.lookUp')}
            </button>
          </form>

          <div className={styles.body} aria-live="polite">
            {lookup.state === 'idle' && <p className={styles.hint}>{t('links.hint')}</p>}
            {lookup.state === 'resolving' && (
              <p className={styles.status}>
                <LoaderCircle size={16} className={styles.spin} aria-hidden="true" />
                {t('links.resolving')}
              </p>
            )}
            {lookup.state === 'failed' && (
              <p className={styles.error} role="alert">
                {lookup.message}
              </p>
            )}
            {preview?.kind === 'track' && (
              <>
                <TrackSummary track={preview.track} />
                <div className={styles.actions}>
                  <button
                    type="button"
                    className={styles.primary}
                    disabled={busy}
                    onClick={play('play')}
                  >
                    <Play size={15} fill="currentColor" aria-hidden="true" />
                    {t('library.play')}
                  </button>
                  <button
                    type="button"
                    className={styles.secondary}
                    disabled={busy}
                    onClick={play('next')}
                  >
                    <ListStart size={15} aria-hidden="true" />
                    {t('library.playNext')}
                  </button>
                  <button
                    type="button"
                    className={styles.secondary}
                    disabled={busy}
                    onClick={play('append')}
                  >
                    <ListPlus size={15} aria-hidden="true" />
                    {t('library.addToQueue')}
                  </button>
                  {!preview.track.live && !preview.track.downloaded && (
                    <button
                      type="button"
                      className={styles.secondary}
                      disabled={busy}
                      onClick={download}
                    >
                      <Download size={15} aria-hidden="true" />
                      {t('links.download')}
                    </button>
                  )}
                </div>
              </>
            )}
            {preview?.kind === 'playlist' && (
              <>
                <div className={styles.summary}>
                  <Thumbnail id={preview.artwork ?? preview.tracks[0]?.artwork ?? null} />
                  <div className={styles.summaryText}>
                    <span className={styles.kind}>{t('library.playlist')}</span>
                    <h3 className={styles.name}>{preview.title}</h3>
                    {preview.uploader && <p className={styles.details}>{preview.uploader}</p>}
                    <p className={styles.meta}>
                      {t('library.songCount', { count: preview.count })}
                      {preview.playlistId !== null && (
                        <span className={styles.badge}>{t('links.imported')}</span>
                      )}
                    </p>
                  </div>
                </div>
                {preview.tracks.length > 0 && (
                  <ol className={styles.entries}>
                    {preview.tracks.map((track, index) => (
                      <li key={`${index}:${track.url}`} className={styles.entry}>
                        <span className={styles.entryNumber}>{index + 1}</span>
                        <span className={styles.entryTitle}>{track.title}</span>
                        <span className={styles.entryArtist}>{track.artist}</span>
                        <span className={styles.entryTime}>
                          {track.duration !== null ? formatTime(track.duration) : ''}
                        </span>
                      </li>
                    ))}
                  </ol>
                )}
                <p className={styles.hint}>{t('links.importHint')}</p>
                <div className={styles.actions}>
                  <button
                    type="button"
                    className={styles.primary}
                    disabled={busy || preview.count === 0}
                    onClick={play('play')}
                  >
                    <Play size={15} fill="currentColor" aria-hidden="true" />
                    {t('library.play')}
                  </button>
                  <button
                    type="button"
                    className={styles.secondary}
                    disabled={busy}
                    onClick={importPlaylist}
                  >
                    <ListMusic size={15} aria-hidden="true" />
                    {t(preview.playlistId === null ? 'links.import' : 'links.openPlaylist')}
                  </button>
                  <button
                    type="button"
                    className={styles.secondary}
                    disabled={busy || preview.count === 0}
                    onClick={download}
                  >
                    <Download size={15} aria-hidden="true" />
                    {t('links.downloadAll')}
                  </button>
                </div>
              </>
            )}
          </div>
        </Glass>
      </motion.div>
    </div>
  );
}
