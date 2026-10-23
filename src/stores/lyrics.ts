import { isTauri } from '@tauri-apps/api/core';
import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { NO_LYRICS, lyricsApi, type CurrentLyrics } from '../lib/lyrics.ts';

/** The playing track's lyrics, mirrored from the backend. */
export const useLyrics = create<{ current: CurrentLyrics }>(() => ({ current: NO_LYRICS }));

export async function initLyrics(): Promise<void> {
  bus.on('lyrics:changed', (current) => useLyrics.setState({ current }));
  if (isTauri()) {
    try {
      useLyrics.setState({ current: await lyricsApi.current() });
    } catch (err) {
      console.error('Reading the lyrics failed', err);
    }
  }
}
