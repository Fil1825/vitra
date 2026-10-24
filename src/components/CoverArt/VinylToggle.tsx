import { Disc3 } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { useSettings } from '../../stores/settings.ts';
import styles from './CoverArt.module.css';
import { useVinyl } from './useVinyl.ts';

/**
 * The disc button on a big cover: turns covers into records, or back. It records the choice
 * in the settings (on or off), over what the theme says. Its parent must be positioned; it
 * shows while the pointer is over the parent.
 */
export function VinylToggle() {
  const { t } = useTranslation();
  const vinyl = useVinyl();
  const update = useSettings((s) => s.update);
  return (
    <button
      type="button"
      className={styles.vinylToggle}
      aria-pressed={vinyl}
      aria-label={t(vinyl ? 'artwork.vinylOff' : 'artwork.vinylOn')}
      title={t(vinyl ? 'artwork.vinylOff' : 'artwork.vinylOn')}
      onClick={(event) => {
        // The cover under it opens the full-size artwork; this button doesn't.
        event.stopPropagation();
        void update((s) => ({
          ...s,
          appearance: { ...s.appearance, vinyl: vinyl ? 'off' : 'on' },
        })).catch((err: unknown) => console.error('Switching vinyl failed', err));
      }}
    >
      <Disc3 size={16} aria-hidden="true" />
    </button>
  );
}
