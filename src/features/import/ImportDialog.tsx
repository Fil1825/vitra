import { open } from '@tauri-apps/plugin-dialog';
import { FileUp, ListMusic, LoaderCircle, Sparkles, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useRef, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import {
  importer,
  isServiceLink,
  PLAYLIST_FILE_EXTENSIONS,
  type ImportResult,
  type MusicPlaylist,
} from '../../lib/import.ts';
import { useEnvironment } from '../../stores/environment.ts';
import { useNavigation } from '../../stores/navigation.ts';
import linkStyles from '../links/LinkDialog.module.css';
import { importError } from './errors.ts';
import styles from './ImportDialog.module.css';

type Tab = 'text' | 'music';

type Status =
  | { state: 'idle' }
  | { state: 'importing' }
  | { state: 'failed'; message: string }
  /** The service showed only part of the list: pasted tracks complete it. */
  | { state: 'partial'; result: ImportResult };

/** Shows the (first) imported playlist. */
function show(results: ImportResult[]): void {
  const [first] = results;
  if (first) {
    useNavigation.getState().go({ view: 'playlist', id: first.playlist.id });
  }
}

/**
 * "Import Playlist…": a link to a list on a service, pasted tracks or a playlist file, and
 * on macOS the Music app's playlists. The playlist appears at once; its missing tracks are
 * looked for in the background (its page shows how far that got).
 */
export function ImportDialog({ initial, onClose }: { initial: string; onClose(): void }) {
  const { t } = useTranslation();
  const mac = useEnvironment((s) => s.platform === 'macos');
  const [tab, setTab] = useState<Tab>('text');
  const [text, setText] = useState(initial);
  const [name, setName] = useState('');
  // A pasted link imports at once (it was pasted to be imported); pasted tracks wait for a
  // name and the button.
  const [status, setStatus] = useState<Status>(() =>
    isServiceLink(initial) ? { state: 'importing' } : { state: 'idle' },
  );
  const field = useRef<HTMLTextAreaElement>(null);
  const busy = status.state === 'importing';
  const into = status.state === 'partial' ? status.result.playlist.id : null;
  const singleLink = isServiceLink(text) || /^https?:\/\/\S+$/i.test(text.trim());

  /** Imports, the status already showing it. */
  const finish = (work: () => Promise<ImportResult[]>) =>
    work().then(
      (results) => {
        show(results);
        const partial = results.find((result) => result.partial);
        if (partial && into === null) {
          setText('');
          setStatus({ state: 'partial', result: partial });
        } else {
          onClose();
        }
      },
      (err: unknown) => setStatus({ state: 'failed', message: importError(err, t) }),
    );

  const run = (work: () => Promise<ImportResult[]>) => {
    setStatus({ state: 'importing' });
    return finish(work);
  };

  const textImport = (value: string) => async () => [
    await importer.text(value, name.trim() || t('import.defaultName'), into),
  ];

  const importText = (value: string) => {
    if (value.trim()) {
      void run(textImport(value));
    }
  };

  useEffect(() => {
    if (isServiceLink(initial)) {
      void finish(textImport(initial));
    } else {
      field.current?.focus();
    }
    // Only what the dialog was opened with.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    importText(text);
  };

  const chooseFiles = async () => {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: t('import.playlistFiles'), extensions: PLAYLIST_FILE_EXTENSIONS }],
    });
    if (picked && picked.length > 0) {
      await run(() => importer.files(picked));
    }
  };

  return (
    <div
      className={linkStyles.backdrop}
      onMouseDown={(e) => e.target === e.currentTarget && onClose()}
    >
      {/* Only transform animates: opacity on an ancestor would cut the glass off its backdrop. */}
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
          aria-labelledby="import-dialog-title"
          priority={7}
        >
          <header className={linkStyles.header}>
            <h2 id="import-dialog-title" className={linkStyles.title}>
              {t('import.title')}
            </h2>
            <IconButton label={t('links.close')} onClick={onClose}>
              <X size={18} />
            </IconButton>
          </header>

          {mac && into === null && (
            <div className={styles.tabs}>
              <SegmentedControl<Tab>
                label={t('import.source')}
                value={tab}
                options={[
                  { value: 'text', label: t('import.tabs.text') },
                  { value: 'music', label: t('import.tabs.music') },
                ]}
                onChange={setTab}
              />
            </div>
          )}

          {tab === 'text' ? (
            <form className={styles.body} onSubmit={submit}>
              {status.state === 'partial' && (
                <p className={styles.notice} role="status">
                  <Sparkles size={16} aria-hidden="true" />
                  {t('import.partial', {
                    name: status.result.playlist.name,
                    count: status.result.total,
                  })}
                </p>
              )}
              <textarea
                ref={field}
                className={styles.text}
                value={text}
                rows={8}
                placeholder={t(into === null ? 'import.placeholder' : 'import.completePlaceholder')}
                aria-label={t('import.textLabel')}
                spellCheck={false}
                disabled={busy}
                onChange={(event) => setText(event.target.value)}
                onKeyDown={(event) => {
                  // ⌘↩ / Ctrl+↩ imports; a plain Enter is a new line.
                  if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                    event.preventDefault();
                    importText(text);
                  }
                }}
              />
              {!singleLink && into === null && (
                <input
                  className={linkStyles.input}
                  value={name}
                  placeholder={t('import.defaultName')}
                  aria-label={t('nav.playlistName')}
                  disabled={busy}
                  onChange={(event) => setName(event.target.value)}
                />
              )}
              {into === null && <p className={linkStyles.hint}>{t('import.hint')}</p>}
              <Status status={status} />
              <div className={styles.actions}>
                {into === null && (
                  <button
                    type="button"
                    className={linkStyles.secondary}
                    disabled={busy}
                    onClick={() => void chooseFiles()}
                  >
                    <FileUp size={15} aria-hidden="true" />
                    {t('import.chooseFile')}
                  </button>
                )}
                <span className={styles.spacer} />
                {into !== null && (
                  <button type="button" className={linkStyles.secondary} onClick={onClose}>
                    {t('import.done')}
                  </button>
                )}
                <button
                  type="submit"
                  className={linkStyles.primary}
                  disabled={busy || !text.trim()}
                >
                  <ListMusic size={15} aria-hidden="true" />
                  {t(into === null ? 'import.import' : 'import.complete')}
                </button>
              </div>
            </form>
          ) : (
            <MusicTab
              busy={busy}
              status={status}
              onImport={(ids) => run(() => importer.music(ids))}
            />
          )}
        </Glass>
      </motion.div>
    </div>
  );
}

function Status({ status }: { status: Status }) {
  const { t } = useTranslation();
  if (status.state === 'importing') {
    return (
      <p className={linkStyles.status} aria-live="polite">
        <LoaderCircle size={16} className={linkStyles.spin} aria-hidden="true" />
        {t('import.reading')}
      </p>
    );
  }
  if (status.state === 'failed') {
    return (
      <p className={linkStyles.error} role="alert">
        {status.message}
      </p>
    );
  }
  return null;
}

type MusicList =
  | { state: 'loading' }
  | { state: 'ready'; playlists: MusicPlaylist[] }
  | { state: 'failed'; message: string };

/** The Music app's playlists to tick and import (macOS). */
function MusicTab({
  busy,
  status,
  onImport,
}: {
  busy: boolean;
  status: Status;
  onImport(ids: string[]): Promise<void>;
}) {
  const { t } = useTranslation();
  const [list, setList] = useState<MusicList>({ state: 'loading' });
  const [chosen, setChosen] = useState<ReadonlySet<string>>(new Set());

  useEffect(() => {
    let current = true;
    importer.musicPlaylists().then(
      (playlists) => current && setList({ state: 'ready', playlists }),
      (err: unknown) => current && setList({ state: 'failed', message: importError(err, t) }),
    );
    return () => {
      current = false;
    };
  }, [t]);

  const toggle = (id: string) =>
    setChosen((set) => {
      const next = new Set(set);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  return (
    <div className={styles.body}>
      {list.state === 'loading' && (
        <p className={linkStyles.status}>
          <LoaderCircle size={16} className={linkStyles.spin} aria-hidden="true" />
          {t('import.music.loading')}
        </p>
      )}
      {list.state === 'failed' && (
        <p className={linkStyles.error} role="alert">
          {list.message}
        </p>
      )}
      {list.state === 'ready' && list.playlists.length === 0 && (
        <p className={linkStyles.hint}>{t('import.music.empty')}</p>
      )}
      {list.state === 'ready' && list.playlists.length > 0 && (
        <ul className={styles.music} aria-label={t('import.tabs.music')}>
          {list.playlists.map((playlist) => (
            <li key={playlist.id}>
              <label className={styles.musicRow}>
                <input
                  type="checkbox"
                  checked={chosen.has(playlist.id)}
                  disabled={busy}
                  onChange={() => toggle(playlist.id)}
                />
                <span className={styles.musicName}>
                  {playlist.name}
                  {playlist.folder && <span className={styles.folder}>{playlist.folder}</span>}
                </span>
                <span className={styles.musicCount}>
                  {t('library.songCount', { count: playlist.tracks })}
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}
      <p className={linkStyles.hint}>{t('import.music.hint')}</p>
      <Status status={status} />
      <div className={styles.actions}>
        {list.state === 'ready' && list.playlists.length > 0 && (
          <button
            type="button"
            className={linkStyles.secondary}
            disabled={busy}
            onClick={() =>
              setChosen(
                chosen.size === list.playlists.length
                  ? new Set()
                  : new Set(list.playlists.map((playlist) => playlist.id)),
              )
            }
          >
            {t(chosen.size === list.playlists.length ? 'import.music.none' : 'import.music.all')}
          </button>
        )}
        <span className={styles.spacer} />
        <button
          type="button"
          className={linkStyles.primary}
          disabled={busy || chosen.size === 0}
          onClick={() => void onImport([...chosen])}
        >
          <ListMusic size={15} aria-hidden="true" />
          {t('import.music.import', { count: chosen.size })}
        </button>
      </div>
    </div>
  );
}
