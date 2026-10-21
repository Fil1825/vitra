import { invoke, isTauri } from '@tauri-apps/api/core';

/** Where a helper binary was found; mirrors `SidecarSource` in `src-tauri/src/sidecar.rs`. */
export type SidecarSource = 'appData' | 'bundled' | 'dev' | 'system';

export interface SidecarStatus {
  name: string;
  path: string | null;
  source: SidecarSource | null;
}

/** Asks the backend which helper binaries (mpv, yt-dlp, ffmpeg, ...) it can find. */
export async function getSidecarStatus(): Promise<SidecarStatus[]> {
  if (!isTauri()) {
    return [];
  }
  return invoke<SidecarStatus[]>('sidecar_status');
}
