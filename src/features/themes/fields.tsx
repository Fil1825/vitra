import { ChevronRight } from 'lucide-react';
import { useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Slider } from '../../components/ui/Slider.tsx';
import { isHexColor, joinColor, percent, splitColor } from '../../theme/color.ts';
import styles from './ThemeEditor.module.css';

/** A part of the editor that opens and closes; the first one starts open. */
export function Section({
  title,
  open: initiallyOpen = false,
  children,
}: {
  title: string;
  open?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(initiallyOpen);
  return (
    <section className={styles.section} data-open={open || undefined}>
      <button
        type="button"
        className={styles.sectionTitle}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
      >
        <ChevronRight size={14} className={styles.chevron} aria-hidden="true" />
        {title}
      </button>
      {open && <div className={styles.sectionBody}>{children}</div>}
    </section>
  );
}

/** A labelled row: the label on the left, the control on the right. */
export function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.field}>
      <span className={styles.label}>{label}</span>
      <div className={styles.control}>{children}</div>
    </div>
  );
}

/** A slider with its value beside it. */
export function NumberField({
  label,
  value,
  min,
  max,
  step = 1,
  unit = '',
  format,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  unit?: string;
  format?(value: number): string;
  onChange(value: number): void;
}) {
  const shown = format ? format(value) : `${Number(value.toFixed(2))}${unit}`;
  return (
    <Field label={label}>
      <Slider
        className={styles.slider}
        label={label}
        value={value}
        min={min}
        max={max}
        step={step}
        valueText={shown}
        onChange={onChange}
      />
      <span className={styles.value}>{shown}</span>
    </Field>
  );
}

/**
 * A theme color: the system color picker for the color, a slider for its opacity, and the hex
 * code to type or paste (taken once it is a valid color).
 */
export function ColorField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange(value: string): void;
}) {
  const { t } = useTranslation();
  const parts = splitColor(value) ?? { rgb: '#000000', alpha: 1 };
  const [typed, setTyped] = useState<string | null>(null);
  return (
    <Field label={label}>
      <span className={styles.colorSwatch}>
        <input
          type="color"
          aria-label={label}
          value={parts.rgb}
          onChange={(event) => onChange(joinColor(event.currentTarget.value, parts.alpha))}
        />
        <span style={{ background: value }} aria-hidden="true" />
      </span>
      <Slider
        className={styles.alpha}
        label={t('themeEditor.opacityOf', { name: label })}
        value={parts.alpha}
        min={0}
        max={1}
        step={0.01}
        valueText={percent(parts.alpha)}
        onChange={(alpha) => onChange(joinColor(parts.rgb, alpha))}
      />
      <input
        className={styles.hex}
        aria-label={t('themeEditor.hexOf', { name: label })}
        spellCheck={false}
        value={typed ?? value}
        onChange={(event) => {
          const text = event.currentTarget.value.trim();
          setTyped(text);
          if (isHexColor(text)) onChange(text.toUpperCase());
        }}
        onBlur={() => setTyped(null)}
      />
    </Field>
  );
}
