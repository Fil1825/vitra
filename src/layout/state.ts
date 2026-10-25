import { create } from 'zustand';

import { useSettings } from '../stores/settings.ts';
import { useUi } from '../stores/ui.ts';
import {
  isShown,
  normalizeLayout,
  setShown,
  shownIn,
  type Layout,
  type ModuleId,
} from './model.ts';

/**
 * The layout lives in the settings (Rust keeps it, every window hears of changes); these
 * helpers read and change it.
 */

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

export function useLayout(): Layout {
  return useSettings((s) => s.settings.layout);
}

export function updateLayout(change: (layout: Layout) => Layout): void {
  void useSettings
    .getState()
    .update((s) => ({ ...s, layout: change(s.layout) }))
    .catch(report('Changing the layout'));
}

/** The sidebar: full, a narrow strip of icons, or gone. */
export type SidebarMode = 'expanded' | 'rail' | 'hidden';

const SIDEBAR_ORDER: readonly SidebarMode[] = ['expanded', 'rail', 'hidden'];

/** The state the sidebar button and shortcut switch to from `mode`. */
export function nextSidebarMode(mode: SidebarMode): SidebarMode {
  return SIDEBAR_ORDER[(SIDEBAR_ORDER.indexOf(mode) + 1) % SIDEBAR_ORDER.length] ?? 'expanded';
}

export function sidebarMode(layout: Layout): SidebarMode {
  return isShown(layout, 'sidebar') ? layout.sidebar : 'hidden';
}

export function withSidebarMode(layout: Layout, mode: SidebarMode): Layout {
  return mode === 'hidden'
    ? setShown(layout, 'sidebar', false)
    : { ...setShown(layout, 'sidebar', true), sidebar: mode };
}

export function setSidebarMode(mode: SidebarMode): void {
  updateLayout((layout) => withSidebarMode(layout, mode));
}

export function setModuleShown(module: ModuleId, shown: boolean): void {
  updateLayout((layout) => setShown(layout, module, shown));
}

/** Shows a module, or hides it if it shows (the queue and lyrics buttons). */
export function toggleModule(module: ModuleId): void {
  updateLayout((layout) => setShown(layout, module, !isShown(layout, module)));
}

/** Shows the full sidebar if needed and focuses its search field. */
export function focusSearch(): void {
  const layout = useSettings.getState().settings.layout;
  if (sidebarMode(layout) !== 'expanded') {
    setSidebarMode('expanded');
  }
  useUi.setState({ searchRequested: true });
}

/** Whether a zone has anything on screen. */
export function zoneShows(layout: Layout, zone: Parameters<typeof shownIn>[1]): boolean {
  return shownIn(layout, zone).length > 0;
}

/** Customize mode: modules can be dragged between zones, shown and hidden. */
export const useCustomizing = create<{ on: boolean }>(() => ({ on: false }));

export function setCustomizing(on: boolean): void {
  useCustomizing.setState({ on });
}

/**
 * Before layouts, the sidebar's mode and the open side panel were kept in localStorage;
 * they move into the layout once, so nobody's window changes after the update.
 */
export function adoptOldPanels(storage: Storage | undefined = globalThis.localStorage): void {
  try {
    if (!storage) return;
    const sidebar = storage.getItem('vitra.sidebar');
    const panel =
      storage.getItem('vitra.sidePanel') ??
      (storage.getItem('vitra.queueOpen') === '1' ? 'queue' : null);
    if (sidebar === null && panel === null) return;
    updateLayout((current) => {
      let layout = normalizeLayout(current);
      if (sidebar === 'expanded' || sidebar === 'rail' || sidebar === 'hidden') {
        layout = withSidebarMode(layout, sidebar);
      }
      if (panel === 'queue' || panel === 'lyrics') {
        layout = setShown(layout, panel, true);
      }
      return layout;
    });
    for (const key of ['vitra.sidebar', 'vitra.sidePanel', 'vitra.queueOpen']) {
      storage.removeItem(key);
    }
  } catch {
    // Storage may be unavailable; the layout simply starts as Vitra's default.
  }
}
