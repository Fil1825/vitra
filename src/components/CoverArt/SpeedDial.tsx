import type { CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';

import { Slider } from '../ui/Slider.tsx';
import styles from './SpeedDial.module.css';
import {
  DETENTS,
  DIAL_MAX,
  DIAL_MIN,
  DIAL_STEP,
  formatSpeed,
  positionOf,
  speedAt,
} from './speedDial.ts';

/**
 * A record speed slider, 5 to 78 turns a minute, with the classic speeds marked under it as
 * detents (speedDial.ts) and the speed written beside it.
 */
export function SpeedDial({
  label,
  speed,
  disabled,
  onChange,
  onCommit,
}: {
  label: string;
  /** Turns a minute. */
  speed: number;
  disabled?: boolean;
  /** Every movement while dragging or pressing arrow keys. */
  onChange(speed: number): void;
  /** When the user lets go. */
  onCommit?(speed: number): void;
}) {
  const { t, i18n } = useTranslation();
  const spoken = t('settings.vinylSpeedValue', { value: formatSpeed(speed, i18n.language) });
  const share = (value: number) => (positionOf(value) - DIAL_MIN) / (DIAL_MAX - DIAL_MIN);
  return (
    <div className={styles.dial} data-disabled={disabled || undefined}>
      <div className={styles.track}>
        <Slider
          label={label}
          value={positionOf(speed)}
          min={DIAL_MIN}
          max={DIAL_MAX}
          step={DIAL_STEP}
          disabled={disabled}
          valueText={spoken}
          onChange={(position) => onChange(speedAt(position))}
          onCommit={onCommit && ((position) => onCommit(speedAt(position)))}
        />
        <div className={styles.detents} aria-hidden="true">
          {DETENTS.map((detent) => (
            <span
              key={detent}
              className={styles.detent}
              style={{ '--_at': share(detent) } as CSSProperties}
            >
              {formatSpeed(detent, i18n.language)}
            </span>
          ))}
        </div>
      </div>
      <span className={styles.value}>{spoken}</span>
    </div>
  );
}
