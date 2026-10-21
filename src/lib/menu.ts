import { isTauri } from '@tauri-apps/api/core';
import { Menu, type MenuItemOptions, type SubmenuOptions } from '@tauri-apps/api/menu';

/** A context menu entry: an action, a submenu, or a separator. */
export type MenuEntry =
  | { label: string; action(): void; enabled?: boolean }
  | { label: string; items: MenuEntry[]; enabled?: boolean }
  | 'separator';

type NativeEntry = MenuItemOptions | SubmenuOptions | { item: 'Separator' };

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
  return { text: entry.label, enabled: entry.enabled ?? true, action: () => action() };
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
