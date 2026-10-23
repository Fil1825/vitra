import { ImagePlus } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { CoverTarget } from '../../lib/library.ts';
import { showContextMenu, type MenuEntry } from '../../lib/menu.ts';
import { useUi } from '../../stores/ui.ts';
import { report } from './actions.ts';
import { Cover } from './Cover.tsx';
import { chooseCover, coverKey, coverMenu } from './covers.ts';
import styles from './Library.module.css';

/**
 * The big cover of an album, artist or playlist page. A button on hover, the context menu
 * and an image file dropped on it change it; with `onView` a click shows it full screen.
 */
export function HeroCover({
  target,
  cover,
  custom,
  shape = 'square',
  onView,
}: {
  target: CoverTarget;
  cover: string | null;
  /** The cover is one the user chose (so it can be reset). */
  custom: boolean;
  shape?: 'square' | 'round';
  onView?(): void;
}) {
  const { t } = useTranslation();
  const key = coverKey(target);
  const dropping = useUi((s) => s.coverDrop === key);
  const picture = <Cover id={cover} shape={shape} className={styles.heroCover} />;

  return (
    <div
      className={styles.heroCoverBox}
      data-cover-drop={key}
      data-dropping={dropping || undefined}
      data-shape={shape}
      onContextMenu={(event) => {
        event.preventDefault();
        const entries: MenuEntry[] = coverMenu(target, custom);
        if (onView && cover) {
          entries.unshift({ label: t('artwork.view'), action: onView }, 'separator');
        }
        showContextMenu(entries).catch(report('Showing a menu'));
      }}
    >
      {onView && cover ? (
        <button
          type="button"
          className={styles.heroCoverButton}
          aria-label={t('artwork.view')}
          onClick={onView}
        >
          {picture}
        </button>
      ) : (
        picture
      )}
      <button
        type="button"
        className={styles.coverEdit}
        aria-label={t('library.changeCover')}
        title={t('library.changeCover')}
        onClick={() => void chooseCover(target).catch(report('Choosing a cover'))}
      >
        <ImagePlus size={16} aria-hidden="true" />
      </button>
    </div>
  );
}
