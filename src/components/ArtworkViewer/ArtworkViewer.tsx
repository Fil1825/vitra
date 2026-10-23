import { ImageOff, X } from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { artwork } from '../../lib/library.ts';
import { artworkUrl } from '../../lib/player.ts';
import type { ArtworkView } from '../../stores/ui.ts';
import { IconButton } from '../ui/IconButton.tsx';
import styles from './ArtworkViewer.module.css';

type Original = { state: 'loading' } | { state: 'ready'; url: string } | { state: 'none' };

/**
 * Artwork over the whole window in its original size (the views show downscaled copies).
 * The copy at hand shows at once; the original replaces it when it's read. Esc, a click
 * anywhere or the close button closes it.
 */
export function ArtworkViewer({ view, onClose }: { view: ArtworkView; onClose(): void }) {
  const { t } = useTranslation();
  const [original, setOriginal] = useState<Original>({ state: 'loading' });

  useEffect(() => {
    let cancelled = false;
    artwork.original(view.source).then(
      (id) => {
        if (!cancelled) {
          setOriginal(id === null ? { state: 'none' } : { state: 'ready', url: artworkUrl(id) });
        }
      },
      (err: unknown) => {
        console.error('Reading the artwork failed', err);
        if (!cancelled) setOriginal({ state: 'none' });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [view.source]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const url = original.state === 'ready' ? original.url : view.preview;

  return (
    <div
      className={styles.backdrop}
      role="dialog"
      aria-modal="true"
      aria-label={t('artwork.view')}
      onClick={onClose}
    >
      <IconButton label={t('artwork.close')} className={styles.close} onClick={onClose}>
        <X size={20} />
      </IconButton>
      {url ? (
        <motion.img
          key={url === view.preview ? 'preview' : 'original'}
          className={styles.image}
          src={url}
          alt=""
          draggable={false}
          initial={{ scale: url === view.preview ? 0.94 : 1 }}
          animate={{ scale: 1 }}
          transition={{ type: 'spring', stiffness: 380, damping: 32 }}
        />
      ) : (
        original.state === 'none' && (
          <p className={styles.missing}>
            <ImageOff size={32} aria-hidden="true" />
            {t('artwork.missing')}
          </p>
        )
      )}
    </div>
  );
}
