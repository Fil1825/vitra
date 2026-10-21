import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Glass } from '../components/Glass/index.ts';
import { formatSpeed } from '../lib/format.ts';
import styles from './ControlBar.module.css';

const SPEEDS = [0.5, 0.75, 1, 1.25, 1.5, 2];

export function SpeedMenu({ speed, onChange }: { speed: number; onChange(speed: number): void }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const close = (event: Event) => {
      if (!root.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (event: KeyboardEvent) => event.key === 'Escape' && setOpen(false);
    document.addEventListener('pointerdown', close);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('pointerdown', close);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div className={styles.speed} ref={root}>
      <button
        type="button"
        className={styles.speedButton}
        aria-haspopup="menu"
        aria-expanded={open}
        title={t('player.speed')}
        onClick={() => setOpen((o) => !o)}
      >
        {formatSpeed(speed)}
      </button>
      {open && (
        <Glass
          className={styles.speedMenu}
          radius="md"
          priority={5}
          role="menu"
          aria-label={t('player.speed')}
        >
          {SPEEDS.map((value) => (
            <button
              key={value}
              type="button"
              role="menuitemradio"
              aria-checked={Math.abs(value - speed) < 0.001}
              className={styles.speedItem}
              onClick={() => {
                onChange(value);
                setOpen(false);
              }}
            >
              {formatSpeed(value)}
            </button>
          ))}
        </Glass>
      )}
    </div>
  );
}
