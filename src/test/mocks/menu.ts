import { vi } from 'vitest';

export interface MenuOptions {
  items?: { text?: string }[];
}

export const popup = vi.fn(async () => {});

async function menuNew(options?: MenuOptions) {
  return { popup, close: vi.fn(async () => {}), options };
}

/** `Menu.new` keeps the items it was given so a test can read the labels. */
export const Menu = {
  new: vi.fn(menuNew),
};

export function resetMenu(): void {
  popup.mockClear();
  Menu.new.mockReset();
  Menu.new.mockImplementation(menuNew);
}
