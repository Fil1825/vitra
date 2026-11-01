import { fireEvent, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';

import { i18n } from '../../i18n/index.ts';
import type { ThemeDraft } from '../../stores/themeEditor.ts';
import { useThemeEditor } from '../../stores/themeEditor.ts';
import { BUILT_IN_THEMES } from '../../theme/themes.ts';
import type { Theme } from '../../theme/schema.ts';
import { onCommand, invoke } from '../../test/mocks/core.ts';
import { ask, message, open, save } from '../../test/mocks/dialog.ts';
import { renderUi } from '../../test/render.ts';
import { ThemeEditor } from './ThemeEditor.tsx';

function makeDraft(): ThemeDraft {
  const light = BUILT_IN_THEMES.find((theme) => theme.id === 'light');
  if (!light) throw new Error('The light theme is missing');
  const theme: Theme = { ...structuredClone(light), id: 'dunes', name: 'Dunes', author: 'Vitra' };
  return { theme, css: null, assets: [], saved: { theme: structuredClone(theme), css: null } };
}

function Editor() {
  const draft = useThemeEditor((s) => s.draft);
  return draft ? <ThemeEditor draft={draft} /> : null;
}

function renderEditor() {
  useThemeEditor.setState({ draft: makeDraft() });
  return renderUi(<Editor />);
}

describe('ThemeEditor', () => {
  it('shows the draft and keeps Save quiet until something changes', async () => {
    await renderEditor();

    expect(screen.getByRole('heading', { name: 'Theme Editor' })).toBeInTheDocument();
    expect(screen.getByDisplayValue('Dunes')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Vitra')).toBeInTheDocument();
    expect(screen.getByRole('radio', { name: 'Light' })).toHaveAttribute('aria-checked', 'true');
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Revert' })).toBeDisabled();
  });

  it('renames the draft, then reverts it', async () => {
    const user = userEvent.setup();
    await renderEditor();

    const name = screen.getByDisplayValue('Dunes');
    await user.clear(name);
    await user.type(name, 'Oasis');
    const author = screen.getByDisplayValue('Vitra');
    await user.clear(author);
    await user.type(author, 'Ada');

    expect(useThemeEditor.getState().draft?.theme.name).toBe('Oasis');
    expect(useThemeEditor.getState().draft?.theme.author).toBe('Ada');
    expect(screen.getByRole('complementary', { name: 'Theme Editor' })).toHaveTextContent(
      'Oasis •',
    );
    expect(screen.getByRole('button', { name: 'Save' })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: 'Revert' }));
    expect(screen.getByDisplayValue('Dunes')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Vitra')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled();
  });

  it('edits appearance, the accent and whether it comes from the artwork', async () => {
    const user = userEvent.setup();
    await renderEditor();

    await user.click(screen.getByRole('radio', { name: 'Dark' }));
    expect(useThemeEditor.getState().draft?.theme.appearance).toBe('dark');
    expect(screen.getByRole('radio', { name: 'Dark' })).toHaveAttribute('aria-checked', 'true');

    await user.click(screen.getByRole('switch', { name: 'Accent from Artwork' }));
    expect(useThemeEditor.getState().draft?.theme.accentFromArtwork).toBe(false);

    const hex = screen.getByRole('textbox', { name: 'Accent hex code' });
    await user.clear(hex);
    await user.type(hex, '#112233');
    expect(useThemeEditor.getState().draft?.theme.colors.accent).toBe('#112233');
  });

  it('saves the draft through the backend and clears the dirty mark', async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.clear(screen.getByDisplayValue('Dunes'));
    await user.type(screen.getByDisplayValue(''), 'Oasis');

    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('theme_save', {
        id: 'dunes',
        json: expect.stringContaining('"name": "Oasis"'),
        css: null,
      }),
    );
    await waitFor(() => expect(screen.getByRole('button', { name: 'Save' })).toBeDisabled());
  });

  it('refuses to save a theme the schema rejects', async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.clear(screen.getByDisplayValue('Dunes'));

    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(message).toHaveBeenCalled());
    expect(String(message.mock.calls[0]?.[0])).toContain('saved');
    expect(invoke).not.toHaveBeenCalledWith('theme_save', expect.anything());
    expect(useThemeEditor.getState().draft?.theme.name).toBe('');
  });

  it('asks before closing a dirty draft and discards it when told', async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.clear(screen.getByDisplayValue('Dunes'));
    await user.type(screen.getByDisplayValue(''), 'Oasis');

    await user.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(useThemeEditor.getState().draft).toBeNull());
    expect(String(ask.mock.calls[0]?.[0])).toContain('Oasis');
    expect(invoke).not.toHaveBeenCalledWith('theme_save', expect.anything());
    expect(screen.queryByRole('heading', { name: 'Theme Editor' })).not.toBeInTheDocument();
  });

  it('saves on close when the dialog says to', async () => {
    const user = userEvent.setup();
    ask.mockResolvedValueOnce(true);
    await renderEditor();
    await user.type(screen.getByDisplayValue('Dunes'), ' plus');

    await user.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'theme_save',
        expect.objectContaining({ id: 'dunes', json: expect.stringContaining('Dunes plus') }),
      ),
    );
    await waitFor(() => expect(useThemeEditor.getState().draft).toBeNull());
  });

  it('closes a clean draft without asking', async () => {
    const user = userEvent.setup();
    await renderEditor();

    await user.click(screen.getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(useThemeEditor.getState().draft).toBeNull());
    expect(ask).not.toHaveBeenCalled();
  });

  it('exports to the path chosen in the save dialog', async () => {
    const user = userEvent.setup();
    save.mockResolvedValueOnce('/tmp/dunes.vitratheme');
    await renderEditor();

    await user.click(screen.getByRole('button', { name: i18n.t('themeEditor.export') }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'theme_export',
        expect.objectContaining({ id: 'dunes', path: '/tmp/dunes.vitratheme' }),
      ),
    );
  });

  it('warns when custom CSS reaches outside the theme', async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.click(screen.getByRole('button', { name: 'Custom CSS' }));

    fireEvent.change(screen.getByRole('textbox', { name: 'Custom CSS' }), {
      target: { value: 'body { background: url(https://evil.test/a.png); }' },
    });

    expect(screen.getByRole('status')).toHaveTextContent('url(https://evil.test/a.png)');
    expect(useThemeEditor.getState().draft?.css).toContain('https://evil.test/a.png');
  });

  it('folds a section away and turns covers into records', async () => {
    const user = userEvent.setup();
    await renderEditor();

    await user.click(screen.getByRole('button', { name: 'Colors' }));
    expect(screen.queryByRole('switch', { name: 'Accent from Artwork' })).not.toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Covers' }));
    await user.click(screen.getByRole('switch', { name: 'Vinyl' }));
    expect(useThemeEditor.getState().draft?.theme.artwork?.vinyl).toBe(true);
    expect(screen.getByRole('slider', { name: 'Record Speed' })).toBeInTheDocument();
  });

  it('copies a chosen picture into the theme', async () => {
    const user = userEvent.setup();
    open.mockResolvedValueOnce('/tmp/cover.png');
    onCommand('theme_add_asset', () => 'assets/cover.png');
    await renderEditor();

    await user.click(screen.getByRole('button', { name: 'Background' }));
    await user.click(screen.getByRole('button', { name: i18n.t('themeEditor.choosePicture') }));

    await waitFor(() =>
      expect(useThemeEditor.getState().draft?.theme.background.image?.src).toBe('assets/cover.png'),
    );
    expect(useThemeEditor.getState().draft?.assets).toContain('cover.png');
    expect(invoke).toHaveBeenCalledWith('theme_add_asset', {
      id: 'dunes',
      path: '/tmp/cover.png',
    });
  });
});
