import { ask, message } from '@tauri-apps/plugin-dialog';

import { i18n } from '../../i18n/index.ts';
import { themesApi } from '../../lib/themes.ts';
import { useThemeEditor, type ThemeDraft } from '../../stores/themeEditor.ts';
import { reloadThemes, useThemes } from '../../stores/themes.ts';
import { themeSchema, type Theme } from '../../theme/schema.ts';
import { describeIssues, serializeTheme } from '../../theme/user.ts';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

/** Opens an installed theme in the editor; the window shows the draft until it closes. */
export function openThemeEditor(id: string): void {
  const user = useThemes.getState().user.find((u) => u.theme.id === id);
  if (!user) {
    console.warn(`No installed theme ${id} to edit`);
    return;
  }
  useThemeEditor.setState({
    draft: {
      theme: user.theme,
      css: user.css,
      assets: user.assets,
      saved: { theme: user.theme, css: user.css },
    },
  });
}

export function isDirty(draft: ThemeDraft): boolean {
  return (
    !draft.saved ||
    serializeTheme(draft.theme) !== serializeTheme(draft.saved.theme) ||
    (draft.css ?? '') !== (draft.saved.css ?? '')
  );
}

/** Changes the draft; every part of the window follows at once. */
export function updateDraft(change: (draft: ThemeDraft) => Partial<ThemeDraft>): void {
  useThemeEditor.setState(({ draft }) => (draft ? { draft: { ...draft, ...change(draft) } } : {}));
}

export function updateTheme(change: (theme: Theme) => Theme): void {
  updateDraft((draft) => ({ theme: change(draft.theme) }));
}

/** What keeps the draft from being saved, or null. */
export function draftProblems(draft: ThemeDraft): string | null {
  const result = themeSchema.safeParse(draft.theme);
  return result.success ? null : describeIssues(result.error);
}

/** Saves the draft into its theme; resolves false if it couldn't. */
export async function saveDraft(): Promise<boolean> {
  const { draft } = useThemeEditor.getState();
  if (!draft) return true;
  const problems = draftProblems(draft);
  if (problems) {
    await message(i18n.t('themeEditor.invalid', { error: problems }), { kind: 'warning' });
    return false;
  }
  try {
    const css = draft.css?.trim() ? draft.css : null;
    await themesApi.save(draft.theme.id, serializeTheme(draft.theme), css);
    await reloadThemes();
    updateDraft(() => ({ css, saved: { theme: draft.theme, css } }));
    return true;
  } catch (err) {
    await message(i18n.t('themeEditor.saveFailed', { error: String(err) }), { kind: 'error' });
    return false;
  }
}

/** Back to the theme as saved. */
export function revertDraft(): void {
  updateDraft((draft) => (draft.saved ? { theme: draft.saved.theme, css: draft.saved.css } : {}));
}

/** Closes the editor, asking first about unsaved changes. */
export async function closeThemeEditor(): Promise<void> {
  const { draft } = useThemeEditor.getState();
  if (draft && isDirty(draft)) {
    const keep = await ask(i18n.t('themeEditor.unsaved', { name: draft.theme.name }), {
      kind: 'warning',
      okLabel: i18n.t('dialog.save'),
      cancelLabel: i18n.t('dialog.discard'),
    });
    if (keep && !(await saveDraft())) return;
  }
  useThemeEditor.setState({ draft: null });
}

/** Copies a picture or font into the theme; resolves with its path (`assets/…`). */
export async function addDraftAsset(path: string): Promise<string | null> {
  const { draft } = useThemeEditor.getState();
  if (!draft) return null;
  try {
    const added = await themesApi.addAsset(draft.theme.id, path);
    const name = added.slice('assets/'.length);
    updateDraft((d) => ({ assets: d.assets.includes(name) ? d.assets : [...d.assets, name] }));
    return added;
  } catch (err) {
    await message(String(err), { kind: 'error' });
    report('Adding a theme asset')(err);
    return null;
  }
}
