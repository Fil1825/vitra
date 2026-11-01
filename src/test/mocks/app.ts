import { vi } from 'vitest';

export const getVersion = vi.fn(async () => '0.1.0');
export const getName = vi.fn(async () => 'Vitra');
export const getTauriVersion = vi.fn(async () => '2.0.0');

export function resetApp(): void {
  getVersion.mockClear();
  getName.mockClear();
  getTauriVersion.mockClear();
}
