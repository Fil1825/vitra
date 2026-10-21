import type { ButtonHTMLAttributes, ReactNode } from 'react';

import styles from './controls.module.css';

export interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Accessible name; also shown as the tooltip. */
  label: string;
  size?: 'regular' | 'large';
  children: ReactNode;
}

export function IconButton({
  label,
  size = 'regular',
  className,
  children,
  ...rest
}: IconButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      className={[styles.iconButton, size === 'large' && styles.large, className]
        .filter(Boolean)
        .join(' ')}
      {...rest}
    >
      {children}
    </button>
  );
}
