import type { TFunction } from 'i18next';
import { describe, expect, it, vi } from 'vitest';

import type { MenuEntry } from '../../lib/menu.ts';
import { DEFAULT_SETTINGS } from '../../lib/settings.ts';
import { miniMenu } from './menu.ts';

const t = ((key: string) => key) as unknown as TFunction;

type Item = Exclude<MenuEntry, 'separator'>;

const find = (entries: MenuEntry[], label: string): Item | undefined =>
  entries.find((entry): entry is Item => entry !== 'separator' && entry.label === label);

describe('miniMenu', () => {
  it('checks the current mode, size and opacity', () => {
    const settings = {
      ...DEFAULT_SETTINGS,
      miniPlayer: { ...DEFAULT_SETTINGS.miniPlayer, mode: 'lyrics' as const, opacity: 0.6 },
    };
    const entries = miniMenu(settings, t, {
      change: vi.fn(),
      showLyrics: vi.fn(),
      expand: vi.fn(),
      close: vi.fn(),
      quit: vi.fn(),
    });
    expect(find(entries, 'mini.modes.lyrics')).toMatchObject({ checked: true });
    expect(find(entries, 'mini.modes.compact')).toMatchObject({ checked: false });
    const size = find(entries, 'mini.size');
    const opacity = find(entries, 'mini.opacity');
    expect(size && 'items' in size && find(size.items, 'mini.sizes.medium')).toMatchObject({
      checked: true,
    });
    expect(opacity && 'items' in opacity && find(opacity.items, '60%')).toMatchObject({
      checked: true,
    });
  });

  it('turns switches the other way and changes the mode', () => {
    const change = vi.fn();
    const showLyrics = vi.fn();
    const entries = miniMenu(DEFAULT_SETTINGS, t, {
      change,
      showLyrics,
      expand: vi.fn(),
      close: vi.fn(),
      quit: vi.fn(),
    });
    const click = (label: string) => {
      const entry = find(entries, label);
      if (entry && 'action' in entry) entry.action();
    };
    click('mini.alwaysOnTop');
    click('mini.clickThrough');
    click('mini.modes.slim');
    click('mini.showLyrics');
    expect(change.mock.calls).toEqual([
      [{ alwaysOnTop: false }],
      [{ clickThrough: true }],
      [{ mode: 'slim' }],
    ]);
    expect(showLyrics.mock.calls).toEqual([[false]]);
  });

  it('offers the wave its like and dislike while one plays', () => {
    const base = {
      change: vi.fn(),
      showLyrics: vi.fn(),
      expand: vi.fn(),
      close: vi.fn(),
      quit: vi.fn(),
    };
    expect(find(miniMenu(DEFAULT_SETTINGS, t, base), 'wave.dislike')).toBeUndefined();
    const dislike = vi.fn();
    const entries = miniMenu(DEFAULT_SETTINGS, t, {
      ...base,
      wave: { favorite: true, like: vi.fn(), dislike },
    });
    expect(entries[0]).toMatchObject({ label: 'wave.like', checked: true });
    const entry = find(entries, 'wave.dislike');
    if (entry && 'action' in entry) entry.action();
    expect(dislike).toHaveBeenCalledOnce();
  });
});
