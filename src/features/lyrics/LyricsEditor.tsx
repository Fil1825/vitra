import { open, save } from '@tauri-apps/plugin-dialog';
import { FileDown, FileUp, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import { SegmentedControl } from '../../components/ui/SegmentedControl.tsx';
import { confirm } from '../../lib/confirm.ts';
import { lyricsApi, type EditorLyrics, type SaveTarget } from '../../lib/lyrics.ts';
import type { LyricsEditorTarget } from '../../stores/ui.ts';
import styles from './LyricsEditor.module.css';
import { startSync, syncedLyrics, type SyncState } from './sync.ts';
import { SyncPane } from './SyncPane.tsx';

type Mode = 'text' | 'sync';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Where saving goes by default: back where the lyrics came from, if possible. */
function defaultTarget(loaded: EditorLyrics): SaveTarget {
  const { source, targets } = loaded;
  if (source === 'tags' && targets.tags) return 'tags';
  if (source === 'file' && targets.file) return 'file';
  if (targets.library) return 'library';
  if (targets.file) return 'file';
  return 'tags';
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function lrcName(path: string): string {
  return fileName(path).replace(/\.[^.]*$/, '') + '.lrc';
}

/** Write, paste or import lyrics for a track, sync them by tapping, and save them. */
export function LyricsEditor({ target, onClose }: { target: LyricsEditorTarget; onClose(): void }) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<EditorLyrics | null>(null);
  const [mode, setMode] = useState<Mode>('text');
  const [text, setText] = useState('');
  const [sync, setSync] = useState<SyncState | null>(null);
  const [tags, setTags] = useState<[string, string][]>([]);
  const [saveTo, setSaveTo] = useState<SaveTarget>('library');
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    lyricsApi.get(target.path, target.libraryId).then(
      (result) => {
        if (!cancelled) {
          setLoaded(result);
          setText(result.text);
          setSaveTo(defaultTarget(result));
        }
      },
      (err: unknown) => !cancelled && setError(String(err)),
    );
    return () => {
      cancelled = true;
    };
  }, [target.path, target.libraryId]);

  const close = useCallback(async () => {
    if (dirty && !(await confirm(t('lyrics.editor.discard'), 'discard'))) {
      return;
    }
    onClose();
  }, [dirty, onClose, t]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        void close();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [close]);

  /** The lyrics as text, from whichever view is showing. */
  const currentText = async (): Promise<{ text: string; untimed: number }> => {
    if (mode === 'text' || !sync) {
      return { text, untimed: 0 };
    }
    const { lyrics, untimed } = syncedLyrics(sync.lines, tags);
    return { text: await lyricsApi.serialize(lyrics), untimed };
  };

  const switchMode = async (next: Mode) => {
    if (next === mode) {
      return;
    }
    setError(null);
    try {
      if (next === 'sync') {
        const lyrics = await lyricsApi.parse(text);
        setTags(lyrics.tags);
        setSync(startSync(lyrics.lines));
      } else {
        setText((await currentText()).text);
      }
      setMode(next);
    } catch (err) {
      setError(String(err));
    }
  };

  const changeSync = (next: SyncState) => {
    setSync(next);
    setDirty(true);
  };

  const importFile = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: t('lyrics.editor.lyricsFiles'), extensions: ['lrc', 'txt'] }],
    });
    if (typeof path !== 'string') {
      return;
    }
    try {
      setText(await lyricsApi.import(path));
      setMode('text');
      setDirty(true);
    } catch (err) {
      setError(String(err));
    }
  };

  const exportFile = async () => {
    const path = await save({
      defaultPath: lrcName(target.path),
      filters: [{ name: t('lyrics.editor.lyricsFiles'), extensions: ['lrc', 'txt'] }],
    });
    if (!path) {
      return;
    }
    try {
      await lyricsApi.export(path, (await currentText()).text);
    } catch (err) {
      setError(String(err));
    }
  };

  const submit = async () => {
    setError(null);
    setBusy(true);
    try {
      const { text: saved } = await currentText();
      if (
        saveTo === 'tags' &&
        !(await confirm(t('lyrics.editor.confirmTags', { file: fileName(target.path) }), 'save'))
      ) {
        setBusy(false);
        return;
      }
      await lyricsApi.save(target.path, target.libraryId, saved, saveTo);
      onClose();
    } catch (err) {
      setError(t('lyrics.editor.saveFailed', { message: String(err) }));
      setBusy(false);
    }
  };

  const targets = loaded?.targets;
  const untimed = mode === 'sync' && sync ? syncedLyrics(sync.lines, tags).untimed : 0;
  const saveHint = {
    tags: t('lyrics.editor.hints.tags'),
    file: t('lyrics.editor.hints.file', { file: lrcName(target.path) }),
    library: t('lyrics.editor.hints.library'),
  }[saveTo];

  return (
    <div
      className={styles.backdrop}
      onMouseDown={(e) => e.target === e.currentTarget && void close()}
    >
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
          aria-labelledby="lyrics-editor-title"
          priority={7}
        >
          <header className={styles.header}>
            <div className={styles.heading}>
              <h2 id="lyrics-editor-title" className={styles.title}>
                {t('lyrics.editor.title')}
              </h2>
              <p className={styles.subtitle} title={target.path}>
                {[target.title, target.artist].filter(Boolean).join(' — ')}
              </p>
            </div>
            <SegmentedControl<Mode>
              label={t('lyrics.editor.mode')}
              value={mode}
              options={[
                { value: 'text', label: t('lyrics.editor.text') },
                { value: 'sync', label: t('lyrics.editor.sync') },
              ]}
              onChange={(next) => void switchMode(next)}
            />
            <IconButton label={t('tags.cancel')} onClick={() => void close()}>
              <X size={18} />
            </IconButton>
          </header>

          <div className={styles.body}>
            {mode === 'text' ? (
              <textarea
                className={styles.textarea}
                value={text}
                disabled={!loaded}
                placeholder={t('lyrics.editor.placeholder')}
                spellCheck={false}
                aria-label={t('lyrics.editor.text')}
                onChange={(event) => {
                  setText(event.target.value);
                  setDirty(true);
                }}
              />
            ) : (
              sync && <SyncPane target={target} state={sync} onChange={changeSync} />
            )}
          </div>

          {untimed > 0 && (
            <p className={styles.warning}>{t('lyrics.editor.untimed', { count: untimed })}</p>
          )}
          {error && (
            <p className={styles.error} role="alert">
              {error}
            </p>
          )}

          <footer className={styles.footer}>
            <div className={styles.files}>
              <button
                type="button"
                className={styles.smallButton}
                onClick={() => void importFile()}
              >
                <FileUp size={14} aria-hidden="true" />
                {t('lyrics.editor.import')}
              </button>
              <button
                type="button"
                className={styles.smallButton}
                onClick={() => void exportFile().catch(report('Exporting lyrics'))}
              >
                <FileDown size={14} aria-hidden="true" />
                {t('lyrics.editor.export')}
              </button>
            </div>
            <div className={styles.saveTo}>
              <span className={styles.saveLabel}>{t('lyrics.editor.saveTo')}</span>
              <SegmentedControl<SaveTarget>
                label={t('lyrics.editor.saveTo')}
                value={saveTo}
                options={[
                  {
                    value: 'tags',
                    label: t('lyrics.editor.targets.tags'),
                    disabled: !targets?.tags,
                  },
                  {
                    value: 'file',
                    label: t('lyrics.editor.targets.file'),
                    disabled: !targets?.file,
                  },
                  {
                    value: 'library',
                    label: t('lyrics.editor.targets.library'),
                    disabled: !targets?.library,
                  },
                ]}
                onChange={setSaveTo}
              />
              <p className={styles.hint}>{saveHint}</p>
            </div>
            <div className={styles.buttons}>
              <button type="button" className={styles.secondary} onClick={() => void close()}>
                {t('tags.cancel')}
              </button>
              <button
                type="button"
                className={styles.primary}
                disabled={!loaded || busy || !targets?.[saveTo]}
                onClick={() => void submit()}
              >
                {busy ? t('tags.saving') : t('tags.save')}
              </button>
            </div>
          </footer>
        </Glass>
      </motion.div>
    </div>
  );
}
