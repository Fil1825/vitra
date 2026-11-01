import { vi } from 'vitest';

/**
 * Stand-in for `@tauri-apps/api/core` in component tests.
 *
 * `isTauri()` is true so the UI takes the same path it takes in the desktop app
 * and `invoke` actually records the command. Unit tests never load this file:
 * their project does not alias the package.
 */
export type CommandArgs = Record<string, unknown> | undefined;

const handlers = new Map<string, (args: CommandArgs) => unknown>();

async function handle(command: string, args?: CommandArgs): Promise<unknown> {
  const custom = handlers.get(command);
  if (custom) return custom(args);
  // Settings are written optimistically and then replaced with whatever the
  // backend stored. Echoing the payload keeps the store on the value just set.
  if (command === 'set_settings') return args?.settings ?? null;
  if (command === 'playlists_list' || command === 'themes_list' || command === 'library_folders') {
    return [];
  }
  return null;
}

export const invoke = vi.fn(handle);

/** Answers one command with `respond` until the mock is reset. */
export function onCommand(command: string, respond: (args: CommandArgs) => unknown): void {
  handlers.set(command, respond);
}

export function resetCore(): void {
  handlers.clear();
  invoke.mockReset();
  invoke.mockImplementation(handle);
}

export function isTauri(): boolean {
  return true;
}

/** Enough of Tauri's asset URL for tests to recognise an id in an `src`. */
export function convertFileSrc(filePath: string, protocol = 'asset'): string {
  return `http://${protocol}.localhost/${encodeURIComponent(filePath)}`;
}
