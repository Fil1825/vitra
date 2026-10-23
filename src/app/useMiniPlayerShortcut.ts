import { useEffect } from 'react';

import { mini } from '../lib/mini.ts';

/**
 * ⌥⌘M / Ctrl+Alt+M switches between the main window and the mini player, in either window
 * (Apple Music's shortcut). Option changes `key` on macOS (µ), so the key itself is matched.
 */
export function useMiniPlayerShortcut(): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;
      if (command && event.altKey && !event.shiftKey && event.code === 'KeyM') {
        event.preventDefault();
        mini.toggle().catch((err: unknown) => console.error('Switching windows failed', err));
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
