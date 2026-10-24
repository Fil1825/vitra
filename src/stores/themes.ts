import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { themesApi } from '../lib/themes.ts';
import { readInstalled, type UserTheme } from '../theme/user.ts';

interface ThemesState {
  /** Installed themes that passed the schema, by name. */
  user: UserTheme[];
  /** Installed themes that didn't, with why (shown in Settings so they can be removed). */
  broken: { id: string; error: string }[];
}

/** Themes the user installed or made, mirrored from the backend. */
export const useThemes = create<ThemesState>(() => ({ user: [], broken: [] }));

export async function reloadThemes(): Promise<void> {
  try {
    const user: UserTheme[] = [];
    const broken: ThemesState['broken'] = [];
    for (const installed of await themesApi.list()) {
      const read = readInstalled(installed);
      if ('error' in read) {
        console.warn(`The theme ${installed.id} is broken:\n${read.error}`);
        broken.push({ id: installed.id, error: read.error });
      } else {
        user.push(read);
      }
    }
    user.sort((a, b) => a.theme.name.localeCompare(b.theme.name));
    useThemes.setState({ user, broken });
  } catch (err) {
    console.error('Reading the installed themes failed', err);
  }
}

export async function initThemes(): Promise<void> {
  bus.on('themes:changed', () => void reloadThemes());
  await reloadThemes();
}
