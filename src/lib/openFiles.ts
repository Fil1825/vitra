import { open } from '@tauri-apps/plugin-dialog';

import { i18n } from '../i18n/index.ts';
import { AUDIO_EXTENSIONS, isAudioFile, player } from './player.ts';

/** Plays the first audio file of a selection or drop (the queue arrives in stage 3). */
export async function playPaths(paths: readonly string[]): Promise<void> {
  const first = paths.find(isAudioFile);
  if (first) {
    await player.open(first);
  }
}

/** Shows the system open dialog and plays the chosen file. */
export async function pickAndPlay(): Promise<void> {
  const selected = await open({
    multiple: true,
    directory: false,
    filters: [{ name: i18n.t('player.audioFiles'), extensions: AUDIO_EXTENSIONS }],
  });
  const paths = selected === null ? [] : Array.isArray(selected) ? selected : [selected];
  await playPaths(paths);
}
