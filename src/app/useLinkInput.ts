import { useEffect } from 'react';

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
 * Ways to open a link: Cmd/Ctrl+U opens the link dialog; pasting a link anywhere outside a
 * text field opens it with that link. (Dropped links come through `useFileDrop`: Tauri keeps
 * drops from the page.)
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

    window.addEventListener('keydown', onKey);
    document.addEventListener('beforepaste', onBeforePaste);
    document.addEventListener('paste', onPaste);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('beforepaste', onBeforePaste);
      document.removeEventListener('paste', onPaste);
    };
  }, []);
}
