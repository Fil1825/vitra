import { Download, Ellipsis, Plus, Trash2 } from 'lucide-react';
import type { CSSProperties, MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { IconButton } from '../../components/ui/IconButton.tsx';
import { showContextMenu, type MenuEntry } from '../../lib/menu.ts';
import { themesApi } from '../../lib/themes.ts';
import { useAppearance } from '../../stores/appearance.ts';
import { useThemes } from '../../stores/themes.ts';
import type { Theme } from '../../theme/schema.ts';
import { AUTO_THEME_ID, BUILT_IN_THEMES } from '../../theme/themes.ts';
import {
  applyThemeLayout,
  duplicateTheme,
  exportTheme,
  pickAndImportThemes,
  removeTheme,
  themeLabel,
} from '../themes/actions.ts';
import { openThemeEditor } from '../themes/editor.ts';
import styles from './SettingsPanel.module.css';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

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

interface Card {
  id: string;
  label: string;
  hint?: string;
  style: CSSProperties;
  theme?: Theme;
  css?: string | null;
  installed?: boolean;
}

/**
 * Auto, the built-in themes and the user's, as cards to pick from. Each theme card has a menu
 * (also on right-click): edit, duplicate, export, use its layout, delete. Below: a new theme
 * (a copy of the current one, opened in the editor) and importing `.vitratheme` files.
 */
export function ThemePicker({ value, onChange }: { value: string; onChange(id: string): void }) {
  const { t } = useTranslation();
  const user = useThemes((s) => s.user);
  const broken = useThemes((s) => s.broken);
  const current = useAppearance((s) => s.theme);

  const cards: Card[] = [
    {
      id: AUTO_THEME_ID,
      label: t('settings.themes.auto'),
      hint: t('settings.autoHint'),
      style: autoSwatchStyle(),
    },
    ...BUILT_IN_THEMES.map((theme) => ({
      id: theme.id,
      label: themeLabel(theme),
      style: swatchStyle(theme),
      theme,
      css: null,
    })),
    ...user.map(({ theme, css }) => ({
      id: theme.id,
      label: theme.name,
      style: swatchStyle(theme),
      theme,
      css,
      installed: true,
    })),
  ];

  const menu = (card: Card): MenuEntry[] => {
    const { theme } = card;
    if (!theme) return [];
    const entries: MenuEntry[] = [];
    if (card.installed) {
      entries.push({ label: t('themes.edit'), action: () => openThemeEditor(theme.id) });
    }
    entries.push(
      {
        label: t('themes.duplicate'),
        action: () =>
          void duplicateTheme(theme, card.css ?? null)
            .then(openThemeEditor)
            .catch(report('Duplicating a theme')),
      },
      {
        label: t('themes.export'),
        action: () => void exportTheme(theme, card.css ?? null).catch(report('Exporting')),
      },
    );
    if (theme.layout) {
      entries.push({ label: t('themes.applyLayout'), action: () => applyThemeLayout(theme) });
    }
    if (card.installed) {
      entries.push('separator', {
        label: t('themes.delete'),
        action: () => void removeTheme(theme.id, theme.name).catch(report('Deleting a theme')),
      });
    }
    return entries;
  };

  const openMenu = (event: MouseEvent, card: Card) => {
    event.preventDefault();
    event.stopPropagation();
    showContextMenu(menu(card)).catch(report('The theme menu'));
  };

  const newTheme = () => {
    // The theme in use, as it looks now: a user theme keeps its CSS in the copy.
    const css = user.find((u) => u.theme.id === current.id)?.css ?? null;
    duplicateTheme(current, css).then(openThemeEditor).catch(report('Creating a theme'));
  };

  return (
    <>
      <div className={styles.themes} role="radiogroup" aria-label={t('settings.theme')}>
        {cards.map((card) => (
          <div
            key={card.id}
            className={styles.themeCell}
            onContextMenu={card.theme ? (event) => openMenu(event, card) : undefined}
          >
            <button
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
            {card.theme && (
              <IconButton
                className={styles.themeMore}
                label={t('themes.more')}
                onClick={(event) => openMenu(event, card)}
              >
                <Ellipsis size={14} />
              </IconButton>
            )}
          </div>
        ))}
      </div>
      <div className={styles.themeActions}>
        <button type="button" className={styles.textButton} onClick={newTheme}>
          <Plus size={14} aria-hidden="true" />
          {t('themes.new')}
        </button>
        <button
          type="button"
          className={styles.textButton}
          onClick={() => void pickAndImportThemes().catch(report('Importing themes'))}
        >
          <Download size={14} aria-hidden="true" />
          {t('themes.import')}
        </button>
      </div>
      {broken.map((theme) => (
        <div key={theme.id} className={styles.brokenTheme} role="alert">
          <div className={styles.rowText}>
            <span>{t('themes.broken', { id: theme.id })}</span>
            <span className={styles.hint}>{theme.error}</span>
          </div>
          <IconButton
            label={t('themes.delete')}
            onClick={() => themesApi.remove(theme.id).catch(report('Deleting a theme'))}
          >
            <Trash2 size={15} />
          </IconButton>
        </div>
      ))}
    </>
  );
}
