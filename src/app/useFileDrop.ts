import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useRef, useState } from 'react';

import { dragLink } from '../lib/links.ts';
import { isAudioFile } from '../lib/player.ts';

/** What is being dragged over the window: something playable from disk, or a web link. */
export type DropKind = 'files' | 'link';

/**
 * Whether a dragged path looks playable: an audio file, or (no extension) most likely a
 * folder. Only decides whether the drop hint shows; the backend sorts out what it can play.
 */
export function isPlayableDrop(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? '';
  return isAudioFile(path) || !name.includes('.');
}

/**
 * Tracks what is dragged over the window. Tauri reports real paths for files (unlike HTML
 * drag and drop) and calls `onDrop` with them. A link dragged from a browser comes as a drop
 * without paths, since Tauri keeps the drop from the page: the backend then reads the link
 * off the drag pasteboard (macOS) and `onLink` gets it. Returns what is being dragged over.
 */
export function useFileDrop(
  onDrop: (paths: string[]) => void,
  onLink: (url: string) => void,
): DropKind | null {
  const [active, setActive] = useState<DropKind | null>(null);
  // Answers for an earlier drag are dropped.
  const drag = useRef(0);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    let unlisten: (() => void) | undefined;
    let disposed = false;
    const report = (err: unknown) => console.error('Reading a dragged link failed', err);
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        switch (payload.type) {
          case 'enter': {
            const current = ++drag.current;
            if (payload.paths.length > 0) {
              setActive(payload.paths.some(isPlayableDrop) ? 'files' : null);
            } else {
              setActive(null);
              dragLink().then((url) => {
                if (url && drag.current === current) setActive('link');
              }, report);
            }
            break;
          }
          case 'leave':
            drag.current++;
            setActive(null);
            break;
          case 'drop': {
            drag.current++;
            setActive(null);
            if (payload.paths.length > 0) {
              onDrop(payload.paths);
            } else {
              dragLink().then((url) => url && onLink(url), report);
            }
            break;
          }
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
  }, [onDrop, onLink]);

  return active;
}
