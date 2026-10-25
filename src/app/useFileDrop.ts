import { isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { useEffect, useRef, useState } from 'react';

import { coverKey, isCoverImage, parseCoverKey } from '../features/library/covers.ts';
import type { CoverTarget } from '../lib/library.ts';
import { dragLink } from '../lib/links.ts';
import { isPlaylistFile } from '../lib/import.ts';
import { isAudioFile } from '../lib/player.ts';
import { isThemeFile } from '../lib/themes.ts';
import { useEnvironment } from '../stores/environment.ts';
import { useUi } from '../stores/ui.ts';

/**
 * What is being dragged over the window: something playable from disk, a web link, an
 * image over a cover that can take it, a `.vitratheme` to install, or playlist files
 * (`.m3u`, the Music app's `.xml`…) to import.
 */
export type DropKind = 'files' | 'link' | 'cover' | 'theme' | 'playlist';

interface Point {
  x: number;
  y: number;
}

/**
 * Tauri's drop position in CSS pixels. Tauri calls it physical, but only Windows reports
 * physical pixels (WebView2 converts the screen point with `ScreenToClient`); macOS
 * (WKWebView's `draggingLocation`, in points) and Linux (GTK widget coordinates) report
 * logical ones, which are CSS pixels already.
 */
export function dropPoint(position: Point, platform: string, ratio: number): Point {
  const scale = platform === 'windows' && ratio > 0 ? ratio : 1;
  return { x: position.x / scale, y: position.y / scale };
}

/** The cover (marked with `data-cover-drop`) under a drop position. */
export function coverAt(position: Point, platform: string, ratio: number): CoverTarget | null {
  const { x, y } = dropPoint(position, platform, ratio);
  const element = document.elementFromPoint(x, y);
  const key = element?.closest<HTMLElement>('[data-cover-drop]')?.dataset.coverDrop;
  return key ? parseCoverKey(key) : null;
}

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
 * off the drag pasteboard (macOS) and `onLink` gets it. An image file dropped on a cover
 * goes to `onCover` instead (the page never sees the drop, so the cover is found by the
 * pointer's position). Returns what is being dragged over.
 */
export function useFileDrop(
  onDrop: (paths: string[]) => void,
  onLink: (url: string) => void,
  onCover: (target: CoverTarget, path: string) => void,
): DropKind | null {
  const [active, setActive] = useState<DropKind | null>(null);
  // Answers for an earlier drag are dropped.
  const drag = useRef(0);
  // The image being dragged, if it is one: only `enter` says what is dragged.
  const image = useRef<string | null>(null);

  useEffect(() => {
    if (!isTauri()) {
      return;
    }
    let unlisten: (() => void) | undefined;
    let disposed = false;
    const report = (err: unknown) => console.error('Reading a dragged link failed', err);
    const coverUnder = (position: Point) =>
      coverAt(position, useEnvironment.getState().platform, window.devicePixelRatio);
    const hover = (target: CoverTarget | null) => {
      const key = target ? coverKey(target) : null;
      if (useUi.getState().coverDrop !== key) {
        useUi.setState({ coverDrop: key });
      }
      return target;
    };
    getCurrentWebview()
      .onDragDropEvent(({ payload }) => {
        switch (payload.type) {
          case 'enter': {
            const current = ++drag.current;
            const [single, ...others] = payload.paths;
            image.current =
              single !== undefined && others.length === 0 && isCoverImage(single) ? single : null;
            if (image.current !== null) {
              setActive(hover(coverUnder(payload.position)) ? 'cover' : null);
            } else if (payload.paths.length > 0) {
              setActive(
                payload.paths.some(isThemeFile)
                  ? 'theme'
                  : payload.paths.some(isPlaylistFile)
                    ? 'playlist'
                    : payload.paths.some(isPlayableDrop)
                      ? 'files'
                      : null,
              );
            } else {
              setActive(null);
              dragLink().then((url) => {
                if (url && drag.current === current) setActive('link');
              }, report);
            }
            break;
          }
          case 'over':
            if (image.current !== null) {
              setActive(hover(coverUnder(payload.position)) ? 'cover' : null);
            }
            break;
          case 'leave':
            drag.current++;
            image.current = null;
            hover(null);
            setActive(null);
            break;
          case 'drop': {
            drag.current++;
            const target = image.current !== null ? coverUnder(payload.position) : null;
            image.current = null;
            hover(null);
            setActive(null);
            const [first] = payload.paths;
            if (target && first !== undefined) {
              onCover(target, first);
            } else if (payload.paths.length > 0) {
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
  }, [onDrop, onLink, onCover]);

  return active;
}
