import { isTauri } from '@tauri-apps/api/core';
import {
  Menu,
  type CheckMenuItemOptions,
  type MenuItemOptions,
  type SubmenuOptions,
} from '@tauri-apps/api/menu';

/**
 * A context menu entry: an action (with a check mark when `checked` is set), a submenu, or a
 * separator.
 */
export type MenuEntry =
  | { label: string; action(): void; enabled?: boolean; checked?: boolean }
  | { label: string; items: MenuEntry[]; enabled?: boolean }
  | 'separator';

type NativeEntry = MenuItemOptions | CheckMenuItemOptions | SubmenuOptions | { item: 'Separator' };

function toNative(entry: MenuEntry): NativeEntry {
  if (entry === 'separator') {
    return { item: 'Separator' };
  }
  if ('items' in entry) {
    return {
      text: entry.label,
      enabled: entry.enabled ?? entry.items.length > 0,
      items: entry.items.map(toNative),
    };
  }
  const { action } = entry;
  const item = { text: entry.label, enabled: entry.enabled ?? true, action: () => action() };
  return entry.checked === undefined ? item : { ...item, checked: entry.checked };
}

/**
 * Shows a native context menu at the pointer. Native menus look right on every platform
 * (and are Liquid Glass on macOS 26) without the WebView having to draw over itself.
 */
export async function showContextMenu(entries: MenuEntry[]): Promise<void> {
  if (!isTauri() || entries.length === 0) {
    return;
  }
  const menu = await Menu.new({ items: entries.map(toNative) });
  await menu.popup();
}
