import { ImagePlus, Trash2, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { IconButton } from '../../components/ui/IconButton.tsx';
import {
  library,
  tags,
  type Album,
  type CoverEdit,
  type TagForm,
  type TagValues,
} from '../../lib/library.ts';
import { pickImage } from '../../lib/openFiles.ts';
import { artworkUrl } from '../../lib/player.ts';
import styles from './TagEditor.module.css';
import { albumTarget } from './album.ts';
import { parseYear } from './year.ts';

type Fields = Omit<TagValues, 'year'> & { year: string };

const FIELDS = ['title', 'artist', 'album', 'albumArtist', 'year'] as const;

/** The basic tag editor: title, artist, album, album artist, year and cover. A link has no
 * file: its details are kept by the library (without a cover). */
export function TagEditor({ path, onClose }: { path: string; onClose(): void }) {
  const { t } = useTranslation();
  const [form, setForm] = useState<TagForm | null>(null);
  const [albums, setAlbums] = useState<Album[]>([]);
  const [fields, setFields] = useState<Fields | null>(null);
  const [cover, setCover] = useState<CoverEdit>({ kind: 'keep' });
  const [preview, setPreview] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    tags.load(path).then(
      (loaded) => {
        if (!cancelled) {
          setForm(loaded);
          setPreview(loaded.cover);
          setFields({
            title: loaded.title,
            artist: loaded.artist,
            album: loaded.album,
            albumArtist: loaded.albumArtist,
            year: loaded.year === null ? '' : String(loaded.year),
          });
        }
      },
      (err: unknown) => !cancelled && setError(String(err)),
    );
    return () => {
      cancelled = true;
    };
  }, [path]);

  // The library's albums, to move the track into one.
  useEffect(() => {
    library
      .albums('artist')
      .then(setAlbums)
      .catch((err: unknown) => console.error('Listing albums failed', err));
  }, []);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const year = fields ? parseYear(fields.year) : null;
  const canSave = form?.writable === true && fields !== null && year !== undefined && !saving;

  const chooseCover = async () => {
    const image = await pickImage();
    if (!image) {
      return;
    }
    try {
      setPreview(await tags.preview(image));
      setCover({ kind: 'replace', path: image });
    } catch (err) {
      setError(String(err));
    }
  };

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!canSave || year === undefined) {
      return;
    }
    setSaving(true);
    setError(null);
    try {
      await tags.save(path, { ...fields, year }, cover);
      onClose();
    } catch (err) {
      setError(t('tags.saveFailed', { message: String(err) }));
      setSaving(false);
    }
  };

  const link = form?.link === true;
  const fileName = link ? path : (path.split(/[\\/]/).pop() ?? path);
  const albumChanged = fields !== null && form !== null && fields.album.trim() !== form.album;
  const target = fields && albumChanged ? albumTarget(fields, albums) : null;
  // Picking an album of the library fills in its artist, so the track goes into it.
  const setAlbum = (value: string) =>
    setFields((current) => {
      if (!current) {
        return current;
      }
      const matches = albums.filter((a) => a.title.toLowerCase() === value.trim().toLowerCase());
      const only = matches.length === 1 ? matches[0] : undefined;
      return {
        ...current,
        album: value,
        albumArtist: only?.artist ?? current.albumArtist,
      };
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
          aria-labelledby="tag-editor-title"
          priority={7}
        >
          <header className={styles.header}>
            <div>
              <h2 id="tag-editor-title" className={styles.title}>
                {link ? t('tags.linkTitle') : t('tags.title')}
              </h2>
              <p className={styles.file} title={path}>
                {fileName}
              </p>
            </div>
            <IconButton label={t('tags.cancel')} onClick={onClose}>
              <X size={18} />
            </IconButton>
          </header>

          <form
            className={styles.body}
            data-link={link || undefined}
            onSubmit={(event) => void submit(event)}
          >
            {!link && (
              <div className={styles.coverColumn}>
                <div className={styles.cover}>
                  {preview ? (
                    <img src={artworkUrl(preview)} alt="" />
                  ) : (
                    <span className={styles.noCover}>{t('tags.noCover')}</span>
                  )}
                </div>
                <div className={styles.coverButtons}>
                  <button
                    type="button"
                    className={styles.smallButton}
                    disabled={!form?.writable}
                    onClick={() => void chooseCover()}
                  >
                    <ImagePlus size={14} aria-hidden="true" />
                    {t('tags.chooseCover')}
                  </button>
                  <button
                    type="button"
                    className={styles.smallButton}
                    disabled={!form?.writable || preview === null}
                    onClick={() => {
                      setPreview(null);
                      setCover({ kind: 'remove' });
                    }}
                  >
                    <Trash2 size={14} aria-hidden="true" />
                    {t('tags.removeCover')}
                  </button>
                </div>
              </div>
            )}

            <div className={styles.fields}>
              {FIELDS.map((name) => (
                <label key={name} className={styles.field}>
                  <span>{t(`tags.fields.${name}`)}</span>
                  <input
                    className={styles.input}
                    value={fields?.[name] ?? ''}
                    disabled={!form?.writable || saving}
                    inputMode={name === 'year' ? 'numeric' : undefined}
                    aria-invalid={name === 'year' && year === undefined}
                    list={name === 'album' ? 'tag-editor-albums' : undefined}
                    onChange={(event) =>
                      name === 'album'
                        ? setAlbum(event.target.value)
                        : setFields((current) =>
                            current ? { ...current, [name]: event.target.value } : current,
                          )
                    }
                  />
                  {name === 'artist' && <span className={styles.hint}>{t('tags.artistHint')}</span>}
                  {name === 'album' && target !== null && fields && (
                    <span className={styles.hint}>
                      {target === 'new'
                        ? t('tags.albumNew', { album: fields.album.trim() })
                        : t('tags.albumExisting', {
                            album: target.title,
                            artist: target.artist ?? t('library.unknownArtist'),
                          })}
                    </span>
                  )}
                </label>
              ))}
              <datalist id="tag-editor-albums">
                {albums.map((album) => (
                  <option key={album.id} value={album.title}>
                    {album.artist ?? ''}
                  </option>
                ))}
              </datalist>
              {link && <p className={styles.hint}>{t('tags.linkHint')}</p>}
              {year === undefined && <p className={styles.hint}>{t('tags.yearInvalid')}</p>}
              {form && !form.writable && <p className={styles.hint}>{t('tags.notWritable')}</p>}
              {error && (
                <p className={styles.error} role="alert">
                  {error}
                </p>
              )}
            </div>

            <footer className={styles.footer}>
              <button type="button" className={styles.secondary} onClick={onClose}>
                {t('tags.cancel')}
              </button>
              <button type="submit" className={styles.primary} disabled={!canSave}>
                {saving ? t('tags.saving') : t('tags.save')}
              </button>
            </footer>
          </form>
        </Glass>
      </motion.div>
    </div>
  );
}
