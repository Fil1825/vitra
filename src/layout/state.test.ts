import { beforeEach, describe, expect, it } from 'vitest';

import { DEFAULT_SETTINGS } from '../lib/settings.ts';
import { useSettings } from '../stores/settings.ts';
import { DEFAULT_LAYOUT, isShown } from './model.ts';
import { adoptOldPanels, nextSidebarMode, sidebarMode, withSidebarMode } from './state.ts';

/** A stand-in for localStorage. */
function storage(items: Record<string, string>): Storage {
  const map = new Map(Object.entries(items));
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (key) => map.get(key) ?? null,
    key: (index) => [...map.keys()][index] ?? null,
    removeItem: (key) => void map.delete(key),
    setItem: (key, value) => void map.set(key, value),
  };
}

beforeEach(() => {
  // No backend in tests: changes stay in the store.
  useSettings.setState({
    settings: DEFAULT_SETTINGS,
    update: async (change) => useSettings.setState((s) => ({ settings: change(s.settings) })),
  });
});

describe('the sidebar mode', () => {
  it('cycles full, icons, hidden', () => {
    expect(nextSidebarMode('expanded')).toBe('rail');
    expect(nextSidebarMode('rail')).toBe('hidden');
    expect(nextSidebarMode('hidden')).toBe('expanded');
  });

  it('is the sidebar module shown or hidden, keeping its width mode', () => {
    const rail = withSidebarMode(DEFAULT_LAYOUT, 'rail');
    expect(sidebarMode(rail)).toBe('rail');
    const hidden = withSidebarMode(rail, 'hidden');
    expect(isShown(hidden, 'sidebar')).toBe(false);
    expect(sidebarMode(hidden)).toBe('hidden');
    expect(sidebarMode(withSidebarMode(hidden, 'expanded'))).toBe('expanded');
  });
});

describe('adoptOldPanels', () => {
  it('moves the sidebar and side panel kept before layouts into the layout, once', () => {
    const store = storage({ 'vitra.sidebar': 'rail', 'vitra.sidePanel': 'lyrics' });
    adoptOldPanels(store);
    const layout = useSettings.getState().settings.layout;
    expect(sidebarMode(layout)).toBe('rail');
    expect(isShown(layout, 'lyrics')).toBe(true);
    expect(isShown(layout, 'queue')).toBe(false);
    expect(store.length).toBe(0);
  });

  it('reads the queue flag of stage 3', () => {
    adoptOldPanels(storage({ 'vitra.queueOpen': '1' }));
    expect(isShown(useSettings.getState().settings.layout, 'queue')).toBe(true);
  });

  it('changes nothing without old values', () => {
    adoptOldPanels(storage({}));
    expect(useSettings.getState().settings.layout).toBe(DEFAULT_SETTINGS.layout);
  });
});
