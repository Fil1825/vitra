import { useEffect, useRef, type CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../../components/Glass/index.ts';
import { bus } from '../../lib/bus.ts';
import styles from './modules.module.css';

/** Bars and how much of the level each one takes, so they don't move as one block. */
const BARS = [0.55, 0.8, 1, 0.9, 0.7, 0.95, 0.6, 0.85, 1, 0.75, 0.5, 0.65];

/**
 * The level meter module: bars that follow the music's loudness (`player:level`, the same
 * signal as the backdrop's pulse). The level goes straight onto the bars' element as a CSS
 * variable, so React doesn't render a dozen times a second.
 */
export function LevelMeter({ className }: { className?: string }) {
  const { t } = useTranslation();
  const bars = useRef<HTMLDivElement>(null);
  useEffect(
    () =>
      bus.on('player:level', (level) => bars.current?.style.setProperty('--_level', String(level))),
    [],
  );
  return (
    <Glass
      as="section"
      className={`${styles.levelMeter} ${className ?? ''}`}
      aria-label={t('layout.modules.level')}
      priority={1}
    >
      <div ref={bars} className={styles.bars} aria-hidden="true">
        {BARS.map((share, index) => (
          <span key={index} className={styles.bar} style={{ '--_share': share } as CSSProperties} />
        ))}
      </div>
    </Glass>
  );
}
