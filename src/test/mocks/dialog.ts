import { vi } from 'vitest';

type AskFn = (message: string, options?: unknown) => Promise<boolean>;
type MessageFn = (message: string, options?: unknown) => Promise<void>;
type OpenFn = (options?: unknown) => Promise<string | string[] | null>;
type SaveFn = (options?: unknown) => Promise<string | null>;

/** Native dialogs. `ask` says no until a test opts in, so closing a dirty editor discards. */
export const ask = vi.fn<AskFn>(async () => false);
export const message = vi.fn<MessageFn>(async () => {});
export const open = vi.fn<OpenFn>(async () => null);
export const save = vi.fn<SaveFn>(async () => null);
export const confirm = vi.fn<AskFn>(async () => false);

export function resetDialog(): void {
  ask.mockReset();
  ask.mockImplementation(async () => false);
  message.mockReset();
  message.mockImplementation(async () => {});
  open.mockReset();
  open.mockImplementation(async () => null);
  save.mockReset();
  save.mockImplementation(async () => null);
  confirm.mockReset();
  confirm.mockImplementation(async () => false);
}
