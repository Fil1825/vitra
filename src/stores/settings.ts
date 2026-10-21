import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { DEFAULT_SETTINGS, loadSettings, saveSettings, type Settings } from '../lib/settings.ts';

interface SettingsState {
  settings: Settings;
  /** Applies a change locally right away, then persists it through the backend. */
  update(change: (current: Settings) => Settings): Promise<void>;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: DEFAULT_SETTINGS,
  async update(change) {
    const previous = get().settings;
    const next = change(previous);
    set({ settings: next });
    try {
      set({ settings: await saveSettings(next) });
    } catch (err) {
      console.error('Saving settings failed', err);
      set({ settings: previous });
    }
  },
}));

/** Loads persisted settings and keeps the store in sync with changes made in other windows. */
export async function initSettings(): Promise<void> {
  useSettings.setState({ settings: await loadSettings() });
  bus.on('settings:changed', (settings) => useSettings.setState({ settings }));
}
