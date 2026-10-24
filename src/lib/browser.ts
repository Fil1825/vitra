import { invoke, isTauri } from '@tauri-apps/api/core';

/** Opens a web page in the default browser (https / http only). */
export async function openInBrowser(url: string): Promise<void> {
  if (isTauri()) await invoke('open_in_browser', { url });
  else window.open(url, '_blank', 'noopener');
}
