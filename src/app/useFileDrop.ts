import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useState } from 'react';

import { isAudioFile } from '../lib/player.ts';

/**
 * Tracks files dragged over the window (Tauri reports real paths, unlike HTML drag & drop)
 * and calls `onDrop` with the dropped paths. Returns whether audio is being dragged over.
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
            setActive(payload.paths.some(isAudioFile));
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
