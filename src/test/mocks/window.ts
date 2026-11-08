import { vi } from 'vitest';

function isMockFn(value: unknown): value is { mockClear: () => void } {
  return (
    typeof value === 'function' &&
    typeof (value as { mockClear?: unknown }).mockClear === 'function'
  );
}

/** The window the WebView thinks it is. Methods the app calls are spies; the rest no-op. */
export const currentWindow = {
  label: 'main',
  listen: vi.fn(async () => () => {}),
  setTheme: vi.fn(async () => {}),
  theme: vi.fn(async () => null),
  scaleFactor: vi.fn(async () => 1),
  innerSize: vi.fn(async () => ({ width: 1280, height: 800 })),
  onResized: vi.fn(async () => () => {}),
  onMoved: vi.fn(async () => () => {}),
  onFocusChanged: vi.fn(async () => () => {}),
  onCloseRequested: vi.fn(async () => () => {}),
  onThemeChanged: vi.fn(async () => () => {}),
  isVisible: vi.fn(async () => true),
  setFocus: vi.fn(async () => {}),
  show: vi.fn(async () => {}),
  hide: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
  setIgnoreCursorEvents: vi.fn(async () => {}),
  setAlwaysOnTop: vi.fn(async () => {}),
  startDragging: vi.fn(async () => {}),
  setTitle: vi.fn(async () => {}),
};

export function getCurrentWindow() {
  return currentWindow;
}

export function getAllWindows() {
  return [currentWindow];
}

export function resetWindow(): void {
  for (const value of Object.values(currentWindow)) {
    if (isMockFn(value)) value.mockClear();
  }
}
