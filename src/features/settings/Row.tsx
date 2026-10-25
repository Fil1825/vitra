import type { ReactNode } from 'react';

import styles from './SettingsPanel.module.css';

interface RowProps {
  label: string;
  hint?: ReactNode;
  /** `inline` puts the control next to the label (switches); `stacked` below it. */
  layout?: 'inline' | 'stacked';
  children: ReactNode;
}

/** A setting: its name, an optional explanation and its control. */
export function Row({ label, hint, layout = 'stacked', children }: RowProps) {
  return (
    <div className={layout === 'inline' ? styles.inlineRow : styles.row}>
      <div className={styles.rowText}>
        <span>{label}</span>
        {hint && <span className={styles.hint}>{hint}</span>}
      </div>
      {children}
    </div>
  );
}

/** Settings that belong together, on a card of their own, with an optional heading. */
export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className={styles.group}>
      {title && <h4 className={styles.groupTitle}>{title}</h4>}
      {children}
    </section>
  );
}
