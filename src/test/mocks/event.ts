import { vi } from 'vitest';

type Handler = (event: { event: string; id: number; payload: unknown }) => void;

const listeners = new Map<string, Set<Handler>>();
let nextId = 1;

/** Records listeners the way `@tauri-apps/api/event` does, and returns an unlisten. */
export const listen = vi.fn(async (event: string, handler: Handler) => {
  let set = listeners.get(event);
  if (!set) {
    set = new Set();
    listeners.set(event, set);
  }
  set.add(handler);
  return () => {
    set.delete(handler);
  };
});

/** Delivers `payload` to every listener currently registered for `event`. */
export const emit = vi.fn(async (event: string, payload?: unknown) => {
  const frame = { event, id: nextId++, payload };
  for (const handler of listeners.get(event) ?? []) handler(frame);
});

export function resetEvents(): void {
  listeners.clear();
  nextId = 1;
  listen.mockClear();
  emit.mockClear();
}
