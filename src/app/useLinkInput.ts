import { useEffect } from 'react';

import { isServiceLink, looksLikeTrackList } from '../lib/import.ts';
import { looksLikeLink } from '../lib/links.ts';
import { useUi } from '../stores/ui.ts';

/** Text fields keep paste for themselves. */
function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest('input, textarea, select') !== null)
  );
}

/**
 * Pasting outside a text field: a link to a music service's list (Spotify, Apple Music…) or
 * several lines of tracks open the Import Playlist dialog with it, any other link the link
 * dialog. (The link dialog's shortcut, ⌘U / Ctrl+U by default, is the `open-link` action;
 * dropped links come through `useFileDrop`: Tauri keeps drops from the page.)
 */
export function useLinkInput(): void {
  useEffect(() => {
    const open = (input: string) => useUi.getState().openLink(input);
    const openImport = (input: string) => useUi.getState().openImport(input);

    // WebKit enables Paste outside text fields only when `beforepaste` is cancelled.
    const onBeforePaste = (event: Event) => {
      if (!isEditable(event.target)) {
        event.preventDefault();
      }
    };
    const onPaste = (event: ClipboardEvent) => {
      const { linkInput, importInput } = useUi.getState();
      if (isEditable(event.target) || linkInput !== null || importInput !== null) {
        return;
      }
      const text = event.clipboardData?.getData('text/plain') ?? '';
      if (isServiceLink(text)) {
        event.preventDefault();
        openImport(text.trim());
      } else if (looksLikeLink(text)) {
        event.preventDefault();
        open(text.trim());
      } else if (looksLikeTrackList(text)) {
        event.preventDefault();
        openImport(text);
      }
    };

    document.addEventListener('beforepaste', onBeforePaste);
    document.addEventListener('paste', onPaste);
    return () => {
      document.removeEventListener('beforepaste', onBeforePaste);
      document.removeEventListener('paste', onPaste);
    };
  }, []);
}
