import { isShown } from '../../layout/model.ts';
import { setModuleShown, toggleModule } from '../../layout/state.ts';
import type { Track } from '../../lib/player.ts';
import { useSettings } from '../../stores/settings.ts';
import { useUi } from '../../stores/ui.ts';

/** Opens the lyrics editor for the playing track. */
export function openLyricsEditor(track: Track): void {
  useUi.getState().editLyrics({
    path: track.path,
    libraryId: track.libraryId,
    title: track.title,
    artist: track.metadata.artist,
  });
}

/**
 * Shows or hides lyrics everywhere: Now Playing, the side panel and the mini player's lyrics
 * mode. A setting, so every window and the next launch follow it.
 */
export function setLyricsVisible(visible: boolean): void {
  useSettings
    .getState()
    .update((s) => ({ ...s, lyrics: { ...s.lyrics, visible } }))
    .catch((err: unknown) => console.error('Showing or hiding lyrics failed', err));
}

/**
 * The lyrics button and shortcut: shows the lyrics module, or hides it. Lyrics hidden
 * everywhere come back in their module, wherever they were hidden.
 */
export function toggleLyrics(): void {
  const { settings } = useSettings.getState();
  if (!settings.lyrics.visible) {
    setLyricsVisible(true);
    if (!isShown(settings.layout, 'lyrics')) setModuleShown('lyrics', true);
  } else {
    toggleModule('lyrics');
  }
}
