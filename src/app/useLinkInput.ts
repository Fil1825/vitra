import { useEffect } from 'react';

import { isRemotePath, looksLikeLink } from '../lib/links.ts';
import { useUi } from '../stores/ui.ts';

/** Text fields keep paste and drop for themselves. */
function isEditable(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || target.closest('input, textarea, select') !== null)
  );
}

/** The first link of a `text/uri-list` (lines starting with `#` are comments). */
export function firstUri(list: string): string | null {
  const line = list
    .split(/\r?\n/)
    .map((entry) => entry.trim())
    .find((entry) => entry !== '' && !entry.startsWith('#'));
  return line ?? null;
}

/**
 * Ways to open a link: Cmd/Ctrl+U opens the link dialog; pasting a link anywhere outside a
 * text field, or dropping one from a browser, opens it with that link.
 */
export function useLinkInput(): void {
  useEffect(() => {
    const open = (input: string) => useUi.getState().openLink(input);

    const onKey = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;
      if (command && !event.shiftKey && !event.altKey && event.key.toLowerCase() === 'u') {
        event.preventDefault();
        open('');
      }
    };
    // WebKit enables Paste outside text fields only when `beforepaste` is cancelled.
    const onBeforePaste = (event: Event) => {
      if (!isEditable(event.target)) {
        event.preventDefault();
      }
    };
    const onPaste = (event: ClipboardEvent) => {
      if (isEditable(event.target) || useUi.getState().linkInput !== null) {
        return;
      }
      const text = event.clipboardData?.getData('text/plain') ?? '';
      if (looksLikeLink(text)) {
        event.preventDefault();
        open(text.trim());
      }
    };
    const draggedLink = (event: DragEvent) =>
      event.dataTransfer?.types.includes('text/uri-list') === true && !isEditable(event.target);
    const onDragOver = (event: DragEvent) => {
      if (draggedLink(event)) {
        event.preventDefault();
      }
    };
    const onDrop = (event: DragEvent) => {
      if (!draggedLink(event)) {
        return;
      }
      const uri = firstUri(event.dataTransfer?.getData('text/uri-list') ?? '');
      if (uri && isRemotePath(uri)) {
        event.preventDefault();
        open(uri);
      }
    };

    window.addEventListener('keydown', onKey);
    document.addEventListener('beforepaste', onBeforePaste);
    document.addEventListener('paste', onPaste);
    document.addEventListener('dragover', onDragOver);
    document.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('beforepaste', onBeforePaste);
      document.removeEventListener('paste', onPaste);
      document.removeEventListener('dragover', onDragOver);
      document.removeEventListener('drop', onDrop);
    };
  }, []);
}
