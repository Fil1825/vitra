import { isTauri } from '@tauri-apps/api/core';
import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { IDLE_SCAN, library, type ScanStatus } from '../lib/library.ts';

/** Progress of folder scans, mirrored from the backend. */
export const useScanStatus = create<{ status: ScanStatus }>(() => ({ status: IDLE_SCAN }));

export async function initLibrary(): Promise<void> {
  bus.on('library:scan', (status) => useScanStatus.setState({ status }));
  if (isTauri()) {
    try {
      useScanStatus.setState({ status: await library.scanStatus() });
    } catch (err) {
      console.error('Reading the scan status failed', err);
    }
  }
}
