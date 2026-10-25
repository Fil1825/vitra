import { LP_SPEED, type Switch } from '../lib/settings.ts';
import type { Theme } from './schema.ts';

/**
 * Whether covers are records: the user's choice (Settings, the disc button on a big cover)
 * or, left on `auto`, the theme's. Records are round and the playing track's turn.
 */
export function vinylOn(theme: Theme, setting: Switch): boolean {
  return setting === 'auto' ? (theme.artwork?.vinyl ?? false) : setting === 'on';
}

/** How fast records turn, in turns a minute: the user's speed, else the theme's, else 33⅓. */
export function vinylSpeed(theme: Theme, setting: number | null): number {
  return setting ?? theme.artwork?.vinylSpeed ?? LP_SPEED;
}
