import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useState } from 'react';

import { isAudioFile } from '../lib/player.ts';

/**
 * Whether a dragged path looks playable: an audio file, or (no extension) most likely a
 * folder. Only decides whether the drop hint shows; the backend sorts out what it can play.
 */
export function isPlayableDrop(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? '';
  return isAudioFile(path) || !name.includes('.');
}

/**
 * Tracks files dragged over the window (Tauri reports real paths, unlike HTML drag & drop)
 * and calls `onDrop` with the dropped paths. Returns whether something playable is being
 * dragged over.
 */
export function useFileDrop(onDrop: (paths: string[]) => void): boolean {
  const [active, setActive] = useState(false);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    let unlisten: (() => void) | undefined;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        switch (payload.type) {
          case 'enter':
            setActive(payload.paths.some(isPlayableDrop));
            break;
          case 'leave':
            setActive(false);
            break;
          case 'drop':
            setActive(false);
            onDrop(payload.paths);
            break;
          default:
            break;
        }
      })
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch((err: unknown) => console.error('Listening for file drops failed', err));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [onDrop]);

  return active;
}
