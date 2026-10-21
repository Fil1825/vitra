import { isTauri } from '@tauri-apps/api/core';
import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { EMPTY_QUEUE, queue, type QueueState } from '../lib/queue.ts';

/** Mirror of the backend's play queue; only the backend changes it (through events). */
export const useQueue = create<{ queue: QueueState }>(() => ({ queue: EMPTY_QUEUE }));

export async function initQueue(): Promise<void> {
  bus.on('queue:changed', (state) => useQueue.setState({ queue: state }));
  if (isTauri()) {
    try {
      useQueue.setState({ queue: await queue.get() });
    } catch (err) {
      console.error('Reading the queue failed', err);
    }
  }
}
