import type { TFunction } from 'i18next';

import type { MenuEntry } from '../../lib/menu.ts';
import { MINI_MODES, MINI_SIZES } from '../../lib/mini.ts';
import type { MiniPlayerSettings, Settings } from '../../lib/settings.ts';

/** Background opacities the menu offers (the settings have a slider). */
export const MENU_OPACITIES = [1, 0.8, 0.6, 0.4] as const;

export interface MiniMenuActions {
  change(change: Partial<MiniPlayerSettings>): void;
  showLyrics(visible: boolean): void;
  expand(): void;
  close(): void;
  quit(): void;
}

/** The mini player's context menu: its mode and window settings, and ways out. */
export function miniMenu(
  { miniPlayer: settings, lyrics }: Pick<Settings, 'miniPlayer' | 'lyrics'>,
  t: TFunction,
  actions: MiniMenuActions,
): MenuEntry[] {
  const percent = Math.round(settings.opacity * 100);
  return [
    ...MINI_MODES.map((mode) => ({
      label: t(`mini.modes.${mode}`),
      checked: settings.mode === mode,
      action: () => actions.change({ mode }),
    })),
    'separator',
    {
      label: t('mini.size'),
      items: MINI_SIZES.map((size) => ({
        label: t(`mini.sizes.${size}`),
        checked: settings.size === size,
        action: () => actions.change({ size }),
      })),
    },
    {
      label: t('mini.opacity'),
      items: MENU_OPACITIES.map((opacity) => ({
        label: `${Math.round(opacity * 100)}%`,
        checked: percent === Math.round(opacity * 100),
        action: () => actions.change({ opacity }),
      })),
    },
    {
      label: t('mini.alwaysOnTop'),
      checked: settings.alwaysOnTop,
      action: () => actions.change({ alwaysOnTop: !settings.alwaysOnTop }),
    },
    {
      label: t('mini.clickThrough'),
      checked: settings.clickThrough,
      action: () => actions.change({ clickThrough: !settings.clickThrough }),
    },
    {
      label: t('mini.showLyrics'),
      checked: lyrics.visible,
      action: () => actions.showLyrics(!lyrics.visible),
    },
    'separator',
    { label: t('mini.expand'), action: actions.expand },
    { label: t('mini.close'), action: actions.close },
    { label: t('mini.quit'), action: actions.quit },
  ];
}
