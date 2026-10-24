import type { Switch } from '../lib/settings.ts';
import type { Theme } from './schema.ts';

/**
 * Whether covers are records: the user's choice (Settings, the disc button on a big cover)
 * or, left on `auto`, the theme's. Records are round and the playing track's turn.
 */
export function vinylOn(theme: Theme, setting: Switch): boolean {
  return setting === 'auto' ? (theme.artwork?.vinyl ?? false) : setting === 'on';
}
