import { Check, LoaderCircle, Play, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { formatTime } from '../../lib/format.ts';
import { displayName, importer, type EntryCandidate, type ImportEntry } from '../../lib/import.ts';
import { links } from '../../lib/links.ts';
import { playTracks, report } from '../library/actions.ts';
import linkStyles from '../links/LinkDialog.module.css';
import { importError } from './errors.ts';
import styles from './FindDialog.module.css';

type Search =
  | { state: 'searching' }
  | { state: 'done'; results: EntryCandidate[] }
  | { state: 'failed'; message: string };

/**
 * "Find…" for an imported track: the library's and YouTube Music's tracks for its names or
 * a typed query; the one chosen takes its place in the playlist.
 */
export function FindDialog({ entry, onClose }: { entry: ImportEntry; onClose(): void }) {
  const { t } = useTranslation();
  const [query, setQuery] = useState(() => displayName(entry.source));
  const [search, setSearch] = useState<Search>({ state: 'searching' });
  const [picking, setPicking] = useState(false);
  const generation = useRef(0);

  /** Searches, the status already showing it. */
  const fetchResults = useCallback(
    (text: string | null) => {
      const current = ++generation.current;
      importer.search(entry.id, text).then(
        (results) => {
          if (generation.current === current) setSearch({ state: 'done', results });
        },
        (err: unknown) => {
          if (generation.current === current) {
            setSearch({ state: 'failed', message: importError(err, t) });
          }
        },
      );
    },
    [entry.id, t],
  );

  // The entry's own names first (the backend also tries them the other way round).
  useEffect(() => fetchResults(null), [fetchResults]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (query.trim()) {
      setSearch({ state: 'searching' });
      fetchResults(query.trim());
    }
  };

  const pick = (candidate: EntryCandidate) => {
    setPicking(true);
    importer.pick(entry.id, candidate).then(onClose, (err: unknown) => {
      setPicking(false);
      setSearch({ state: 'failed', message: importError(err, t) });
    });
  };

  const listen = (candidate: EntryCandidate) => {
    if (candidate.trackId !== null) {
      playTracks([candidate.trackId], 0);
    } else if (candidate.url) {
      links.play(candidate.url, 'play').catch(report('Playing a link'));
    }
  };

  const { source } = entry;
  const wanted = [source.album, source.duration !== null && formatTime(source.duration)]
    .filter(Boolean)
    .join(' · ');

  return (
    <div
      className={linkStyles.backdrop}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      <motion.div
        initial={{ y: 24, scale: 0.98 }}
        animate={{ y: 0, scale: 1 }}
        transition={{ type: 'spring', stiffness: 420, damping: 34 }}
      >
        <Glass
          as="section"
          className={linkStyles.dialog}
          role="dialog"
          aria-modal="true"
          aria-labelledby="find-dialog-title"
          priority={7}
        >
          <header className={linkStyles.header}>
            <h2 id="find-dialog-title" className={linkStyles.title}>
              {t('import.find.title')}
            </h2>
            <IconButton label={t('links.close')} onClick={onClose}>
              <X size={18} />
            </IconButton>
          </header>
          <div className={styles.wanted}>
            <span className={linkStyles.kind}>{t('import.find.inList')}</span>
            <span className={styles.wantedName}>{displayName(source)}</span>
            {wanted && <span className={styles.wantedMeta}>{wanted}</span>}
          </div>
          <form className={linkStyles.form} onSubmit={submit}>
            <input
              className={linkStyles.input}
              value={query}
              aria-label={t('import.find.query')}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setQuery(event.target.value)}
            />
            <button
              type="submit"
              className={linkStyles.secondary}
              disabled={!query.trim() || search.state === 'searching'}
            >
              {t('import.find.search')}
            </button>
          </form>
          <div className={linkStyles.body} aria-live="polite">
            {search.state === 'searching' && (
              <p className={linkStyles.status}>
                <LoaderCircle size={16} className={linkStyles.spin} aria-hidden="true" />
                {t('import.find.searching')}
              </p>
            )}
            {search.state === 'failed' && (
              <p className={linkStyles.error} role="alert">
                {search.message}
              </p>
            )}
            {search.state === 'done' && search.results.length === 0 && (
              <p className={linkStyles.hint}>{t('import.find.nothing')}</p>
            )}
            {search.state === 'done' && search.results.length > 0 && (
              <ul className={styles.results}>
                {search.results.map((candidate) => (
                  <li key={`${candidate.trackId ?? ''}:${candidate.url ?? ''}`}>
                    <div className={styles.result}>
                      <IconButton label={t('import.find.listen')} onClick={() => listen(candidate)}>
                        <Play size={14} fill="currentColor" />
                      </IconButton>
                      <span className={styles.resultText}>
                        <span className={styles.resultTitle}>{candidate.title}</span>
                        <span className={styles.resultMeta}>
                          {[candidate.artists.join(', '), candidate.album]
                            .filter(Boolean)
                            .join(' · ')}
                        </span>
                      </span>
                      <span className={styles.resultSide}>
                        <span className={linkStyles.badge}>
                          {candidate.trackId !== null
                            ? t('links.inLibrary')
                            : t('import.find.youtubeMusic')}
                        </span>
                        {candidate.duration !== null && (
                          <span className={styles.resultTime}>
                            {formatTime(candidate.duration)}
                          </span>
                        )}
                      </span>
                      <button
                        type="button"
                        className={linkStyles.secondary}
                        disabled={picking}
                        onClick={() => pick(candidate)}
                      >
                        <Check size={15} aria-hidden="true" />
                        {t('import.find.choose')}
                      </button>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </Glass>
      </motion.div>
    </div>
  );
}
