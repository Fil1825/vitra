import type { CSSProperties, KeyboardEvent, PointerEvent } from 'react';

import styles from './controls.module.css';

export interface SliderProps {
  label: string;
  value: number;
  min?: number;
  max: number;
  step?: number;
  disabled?: boolean;
  /** Spoken value, e.g. "1:05 of 3:20". */
  valueText?: string;
  className?: string;
  /** Every movement while dragging or pressing arrow keys. */
  onChange(value: number): void;
  /** When the user lets go (pointer up or key up). */
  onCommit?(value: number): void;
}

/** A native range input styled with theme variables: accessible and keyboard-friendly. */
export function Slider({
  label,
  value,
  min = 0,
  max,
  step = 0.01,
  disabled,
  valueText,
  className,
  onChange,
  onCommit,
}: SliderProps) {
  const span = max - min;
  const fill = span > 0 ? ((Math.min(Math.max(value, min), max) - min) / span) * 100 : 0;
  const commit = (event: PointerEvent<HTMLInputElement> | KeyboardEvent<HTMLInputElement>) =>
    onCommit?.(Number(event.currentTarget.value));

  return (
    <input
      type="range"
      className={className ? `${styles.slider} ${className}` : styles.slider}
      style={{ '--_fill': `${fill}%` } as CSSProperties}
      aria-label={label}
      aria-valuetext={valueText}
      min={min}
      max={max}
      step={step}
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(Number(event.currentTarget.value))}
      onPointerUp={commit}
      onKeyUp={commit}
    />
  );
}
