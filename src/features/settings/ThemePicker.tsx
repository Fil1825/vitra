import type { CSSProperties } from 'react';
import { useTranslation } from 'react-i18next';

import type { Theme } from '../../theme/schema.ts';
import { AUTO_THEME_ID, BUILT_IN_THEMES } from '../../theme/themes.ts';
import styles from './SettingsPanel.module.css';

/** Inline preview colors come from theme data, so the card shows the theme, not the current one. */
function swatchStyle(theme: Theme): CSSProperties {
  const [a, b, c] = [...theme.background.gradient, ...theme.background.gradient];
  return {
    background: [
      `radial-gradient(circle at 20% 25%, ${a}, transparent 60%)`,
      `radial-gradient(circle at 80% 40%, ${b}, transparent 60%)`,
      `radial-gradient(circle at 50% 100%, ${c}, transparent 60%)`,
      theme.colors.background,
    ].join(', '),
    color: theme.colors.accent,
  };
}

function autoSwatchStyle(): CSSProperties {
  const light = BUILT_IN_THEMES.find((t) => t.id === 'light');
  const dark = BUILT_IN_THEMES.find((t) => t.id === 'dark');
  return {
    background: `linear-gradient(135deg, ${light?.colors.background} 50%, ${dark?.colors.background} 50%)`,
    color: light?.colors.accent,
  };
}

export function ThemePicker({ value, onChange }: { value: string; onChange(id: string): void }) {
  const { t } = useTranslation();
  const label = (theme: Theme) => {
    switch (theme.id) {
      case 'light':
        return t('settings.themes.light');
      case 'dark':
        return t('settings.themes.dark');
      case 'aurora-night':
        return t('settings.themes.aurora-night');
      default:
        return theme.name;
    }
  };

  const cards = [
    {
      id: AUTO_THEME_ID,
      label: t('settings.themes.auto'),
      hint: t('settings.autoHint'),
      style: autoSwatchStyle(),
    },
    ...BUILT_IN_THEMES.map((theme) => ({
      id: theme.id,
      label: label(theme),
      hint: undefined,
      style: swatchStyle(theme),
    })),
  ];

  return (
    <div className={styles.themes} role="radiogroup" aria-label={t('settings.theme')}>
      {cards.map((card) => (
        <button
          key={card.id}
          type="button"
          role="radio"
          aria-checked={card.id === value}
          className={styles.themeCard}
          onClick={() => onChange(card.id)}
          title={card.hint}
        >
          <span className={styles.swatch} style={card.style}>
            <span className={styles.swatchAccent} />
          </span>
          <span className={styles.themeName}>{card.label}</span>
        </button>
      ))}
    </div>
  );
}
