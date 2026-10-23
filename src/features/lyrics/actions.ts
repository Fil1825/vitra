import type { Track } from '../../lib/player.ts';
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
