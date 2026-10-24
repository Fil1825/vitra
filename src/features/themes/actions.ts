import { ask, message, open, save } from '@tauri-apps/plugin-dialog';

import { i18n } from '../../i18n/index.ts';
import { normalizeLayout } from '../../layout/model.ts';
import { confirm } from '../../lib/confirm.ts';
import { THEME_EXTENSION, themesApi } from '../../lib/themes.ts';
import { useSettings } from '../../stores/settings.ts';
import { reloadThemes, useThemes } from '../../stores/themes.ts';
import { sanitizeCss } from '../../theme/css.ts';
import type { Theme } from '../../theme/schema.ts';
import { BUILT_IN_THEMES, isBuiltIn } from '../../theme/themes.ts';
import { checkThemeJson, serializeTheme, uniqueThemeId, withAssets } from '../../theme/user.ts';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Ids in use: the built-in themes' and the installed ones'. */
function takenIds(): Set<string> {
  return new Set([
    ...BUILT_IN_THEMES.map((theme) => theme.id),
    ...useThemes.getState().user.map((user) => user.theme.id),
    ...useThemes.getState().broken.map((broken) => broken.id),
  ]);
}

/** What a theme is called in the picker: built-in themes have translated names. */
export function themeLabel(theme: Theme): string {
  switch (theme.id) {
    case 'light':
      return i18n.t('settings.themes.light');
    case 'dark':
      return i18n.t('settings.themes.dark');
    case 'aurora-night':
      return i18n.t('settings.themes.aurora-night');
    default:
      return theme.name;
  }
}

export function selectTheme(id: string): void {
  void useSettings
    .getState()
    .update((s) => ({ ...s, appearance: { ...s.appearance, theme: id } }))
    .catch(report('Choosing a theme'));
}

/** Uses the layout a theme suggests in place of the current one. */
export function applyThemeLayout(theme: Theme): void {
  if (!theme.layout) return;
  const layout = normalizeLayout(theme.layout);
  void useSettings
    .getState()
    .update((s) => ({ ...s, layout }))
    .catch(report('Applying the layout'));
}

/** After a theme with a layout arrived: the user decides whether the window changes. */
async function offerLayout(theme: Theme): Promise<void> {
  if (!theme.layout) return;
  const yes = await ask(i18n.t('themes.layoutQuestion', { name: theme.name }), {
    kind: 'info',
    okLabel: i18n.t('themes.applyLayout'),
    cancelLabel: i18n.t('themes.keepLayout'),
  });
  if (yes) applyThemeLayout(theme);
}

/** Asks for `.vitratheme` files and imports them. */
export async function pickAndImportThemes(): Promise<void> {
  const picked = await open({
    multiple: true,
    filters: [{ name: i18n.t('themes.fileType'), extensions: [THEME_EXTENSION] }],
  });
  for (const path of Array.isArray(picked) ? picked : picked ? [picked] : []) {
    await importThemeFile(path);
  }
}

/**
 * Imports a `.vitratheme`: the backend reads it safely, the schema checks `theme.json`, the
 * CSS is cleaned, and the theme is installed and chosen. A theme with a built-in id gets
 * another; one with the id of an installed theme replaces it or is kept beside it.
 */
export async function importThemeFile(path: string): Promise<void> {
  let opened;
  try {
    opened = await themesApi.open(path);
  } catch (err) {
    await message(i18n.t('themes.importFailed', { error: String(err) }), { kind: 'error' });
    return;
  }
  const checked = checkThemeJson(opened.json);
  if (!checked.ok) {
    themesApi.discard(opened.token).catch(report('Discarding a theme'));
    await message(i18n.t('themes.invalid', { error: checked.error }), { kind: 'error' });
    return;
  }

  const taken = takenIds();
  let id = checked.theme.id;
  if (isBuiltIn(id)) {
    id = uniqueThemeId(`${id}-custom`, taken);
  } else if (taken.has(id)) {
    const replace = await ask(i18n.t('themes.replaceQuestion', { name: checked.theme.name }), {
      kind: 'warning',
      okLabel: i18n.t('themes.replace'),
      cancelLabel: i18n.t('themes.keepBoth'),
    });
    if (!replace) id = uniqueThemeId(id, taken);
  }

  const theme = withAssets({ ...checked.theme, id }, opened.assets);
  // Stored clean, with asset paths as they are (they become addresses only when applied).
  const css =
    opened.css === null
      ? null
      : sanitizeCss(opened.css, (asset) =>
          opened.assets.includes(asset.slice('assets/'.length)) ? asset : null,
        );
  try {
    await themesApi.install(opened.token, id, serializeTheme(theme), css?.css || null);
  } catch (err) {
    await message(i18n.t('themes.importFailed', { error: String(err) }), { kind: 'error' });
    return;
  }
  await reloadThemes();
  selectTheme(id);
  if (css && css.removed.length > 0) {
    await message(i18n.t('themes.cssCleaned', { list: css.removed.slice(0, 6).join('\n') }), {
      kind: 'warning',
    });
  }
  await offerLayout(theme);
}

/** Saves a theme as a `.vitratheme` file the user picks. */
export async function exportTheme(theme: Theme, css: string | null): Promise<void> {
  const path = await save({
    defaultPath: `${theme.id}.${THEME_EXTENSION}`,
    filters: [{ name: i18n.t('themes.fileType'), extensions: [THEME_EXTENSION] }],
  });
  if (!path) return;
  try {
    await themesApi.export(theme.id, serializeTheme(theme), css, path);
  } catch (err) {
    await message(i18n.t('themes.exportFailed', { error: String(err) }), { kind: 'error' });
  }
}

/**
 * A copy of a theme to change: built-in ones can't be edited, and a copy keeps the original.
 * The copy is chosen; resolves with its id.
 */
export async function duplicateTheme(theme: Theme, css: string | null): Promise<string> {
  const id = uniqueThemeId(`${theme.id}-copy`, takenIds());
  const copy: Theme = {
    ...theme,
    id,
    name: i18n.t('themes.copyName', { name: themeLabel(theme) }),
  };
  const installed = useThemes.getState().user.some((user) => user.theme.id === theme.id);
  if (installed) {
    await themesApi.duplicate(theme.id, id, serializeTheme(copy));
  } else {
    await themesApi.save(id, serializeTheme(copy), css);
  }
  await reloadThemes();
  selectTheme(id);
  return id;
}

/** Removes an installed theme after asking; the window goes back to Auto if it was in use. */
export async function removeTheme(id: string, name: string): Promise<void> {
  if (!(await confirm(i18n.t('themes.removeQuestion', { name }), 'delete'))) return;
  if (useSettings.getState().settings.appearance.theme === id) {
    selectTheme('auto');
  }
  try {
    await themesApi.remove(id);
    await reloadThemes();
  } catch (err) {
    await message(String(err), { kind: 'error' });
  }
}
