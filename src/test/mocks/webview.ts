import { vi } from 'vitest';

import { getCurrentWindow } from './window.ts';

const onDragDropEvent = vi.fn(async () => () => {});

/** The webview handle file-drop listens on. */
export function getCurrentWebview() {
  return { onDragDropEvent, window: getCurrentWindow() };
}

export function resetWebview(): void {
  onDragDropEvent.mockClear();
}
