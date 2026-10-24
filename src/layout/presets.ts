import { DEFAULT_LAYOUT, normalizeLayout, type Layout } from './model.ts';

/** Layouts Vitra offers by name (translated as `layout.presets.<id>`). */
export const BUILT_IN_PRESETS = [
  { id: 'classic', layout: DEFAULT_LAYOUT },
  {
    // Nothing but the music and the controls.
    id: 'focus',
    layout: normalizeLayout({
      ...DEFAULT_LAYOUT,
      hidden: ['sidebar', 'queue', 'lyrics', 'artwork', 'level'],
    }),
  },
  {
    id: 'lyrics',
    layout: normalizeLayout({
      ...DEFAULT_LAYOUT,
      hidden: ['queue', 'artwork', 'level'],
      widths: { left: 232, right: 420 },
    }),
  },
  {
    // The cover under the sidebar, the queue beside the main view.
    id: 'artwork',
    layout: normalizeLayout({
      ...DEFAULT_LAYOUT,
      zones: {
        left: ['sidebar', 'artwork'],
        right: ['queue', 'lyrics'],
        top: [],
        bottom: ['controls', 'level'],
      },
      hidden: ['lyrics', 'level'],
      weights: { artwork: 0.8 },
    }),
  },
  {
    // The controls along the top, the level meter beside them, the sidebar as icons.
    id: 'topBar',
    layout: normalizeLayout({
      ...DEFAULT_LAYOUT,
      zones: {
        left: ['sidebar'],
        right: ['queue', 'lyrics', 'artwork'],
        top: ['controls', 'level'],
        bottom: [],
      },
      hidden: ['queue', 'lyrics', 'artwork'],
      sidebar: 'rail',
    }),
  },
] as const satisfies readonly { id: string; layout: Layout }[];

export type PresetId = (typeof BUILT_IN_PRESETS)[number]['id'];
