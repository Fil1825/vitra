import { open } from '@tauri-apps/plugin-dialog';

import { i18n } from '../i18n/index.ts';
import { AUDIO_EXTENSIONS, player } from './player.ts';

/** Plays dropped or chosen files and folders, replacing the queue (folders play recursively). */
export async function playPaths(paths: readonly string[]): Promise<void> {
  if (paths.length > 0) {
    await player.open([...paths]);
  }
}

/** Shows the system open dialog and plays the chosen files. */
export async function pickAndPlay(): Promise<void> {
  const selected = await open({
    multiple: true,
    directory: false,
    filters: [{ name: i18n.t('player.audioFiles'), extensions: AUDIO_EXTENSIONS }],
  });
  const paths = selected === null ? [] : Array.isArray(selected) ? selected : [selected];
  await playPaths(paths);
}

/** Shows the system folder dialog; returns the chosen folder. */
export async function pickFolder(): Promise<string | null> {
  const selected = await open({ multiple: false, directory: true });
  return typeof selected === 'string' ? selected : null;
}

/** Shows the system open dialog for a cover image. */
export async function pickImage(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: i18n.t('tags.images'), extensions: ['jpg', 'jpeg', 'png'] }],
  });
  return typeof selected === 'string' ? selected : null;
}
