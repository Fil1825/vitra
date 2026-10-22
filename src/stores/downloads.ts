import { isTauri } from '@tauri-apps/api/core';
import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { downloads, type DownloadJob } from '../lib/links.ts';

/** The download queue, mirrored from the backend. */
export const useDownloads = create<{ jobs: DownloadJob[] }>(() => ({ jobs: [] }));

export async function initDownloads(): Promise<void> {
  bus.on('downloads:changed', (jobs) => useDownloads.setState({ jobs }));
  if (isTauri()) {
    try {
      useDownloads.setState({ jobs: await downloads.list() });
    } catch (err) {
      console.error('Reading the downloads failed', err);
    }
  }
}
