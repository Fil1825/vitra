import { z } from 'zod';

/**
 * The window's layout: modules placed in zones around the main view (library, Now Playing),
 * which always stays in the middle. Columns (left, right) hold tall modules, bars (top,
 * bottom) wide ones. A layout is plain data: the settings keep the current one and its
 * presets, and a theme may carry one.
 */

export const MODULES = ['sidebar', 'queue', 'lyrics', 'artwork', 'controls', 'level'] as const;
export type ModuleId = (typeof MODULES)[number];

export const ZONES = ['left', 'right', 'top', 'bottom'] as const;
export type Zone = (typeof ZONES)[number];

export const COLUMNS = ['left', 'right'] as const;
export type Column = (typeof COLUMNS)[number];

/** Where each module fits: a column, a bar, or (the small level meter) anywhere. */
export const FITS: Record<ModuleId, readonly Zone[]> = {
  sidebar: COLUMNS,
  queue: COLUMNS,
  lyrics: COLUMNS,
  artwork: COLUMNS,
  controls: ['top', 'bottom'],
  level: ZONES,
};

/** A column's width in px, dragged at its inner edge. */
export const COLUMN_WIDTH = { min: 180, max: 560 } as const;

/** A module's share of its zone, next to the others there (dragged between them). */
export const WEIGHT = { min: 0.25, max: 4 } as const;

const moduleId = z.enum(MODULES);
const zoneModules = z.array(moduleId).max(MODULES.length);
const columnWidth = z.number().min(COLUMN_WIDTH.min).max(COLUMN_WIDTH.max);

export const layoutSchema = z.object({
  /** Modules in each zone, in order (top to bottom in a column, left to right in a bar). */
  zones: z.object({ left: zoneModules, right: zoneModules, top: zoneModules, bottom: zoneModules }),
  /** Placed but not shown; the queue and lyrics buttons show and hide theirs. */
  hidden: z.array(moduleId).max(MODULES.length),
  widths: z.object({ left: columnWidth, right: columnWidth }),
  weights: z.partialRecord(moduleId, z.number().min(WEIGHT.min).max(WEIGHT.max)),
  /** The full sidebar or its strip of icons. */
  sidebar: z.enum(['expanded', 'rail']),
});

export type Layout = z.infer<typeof layoutSchema>;
export type Zones = Layout['zones'];

/** The layout Vitra starts with: the one it had before layouts could change. */
export const DEFAULT_LAYOUT: Layout = {
  zones: {
    left: ['sidebar'],
    right: ['queue', 'lyrics', 'artwork'],
    top: [],
    bottom: ['controls', 'level'],
  },
  hidden: ['queue', 'lyrics', 'artwork', 'level'],
  widths: { left: 232, right: 320 },
  weights: {},
  sidebar: 'expanded',
};

/** Where a module goes when a layout doesn't place it (or places it where it can't be). */
export function homeZone(module: ModuleId): Zone {
  for (const zone of ZONES) {
    if (DEFAULT_LAYOUT.zones[zone].includes(module)) {
      return zone;
    }
  }
  return 'bottom';
}

const clamp = (value: number, { min, max }: { min: number; max: number }) =>
  Math.min(max, Math.max(min, value));

/**
 * Makes any layout-like value usable: unknown modules and repeats are dropped, a module in a
 * zone it doesn't fit goes home, missing modules join their home zone hidden, sizes are
 * brought within limits. Settings and themes both pass through here.
 */
export function normalizeLayout(value: unknown): Layout {
  const input = (typeof value === 'object' && value !== null ? value : {}) as Partial<
    Record<keyof Layout, unknown>
  >;
  const isModule = (item: unknown): item is ModuleId => moduleId.safeParse(item).success;
  const zonesIn = (input.zones ?? {}) as Partial<Record<Zone, unknown>>;

  const zones: Zones = { left: [], right: [], top: [], bottom: [] };
  const placed = new Set<ModuleId>();
  const misplaced: ModuleId[] = [];
  for (const zone of ZONES) {
    const items = Array.isArray(zonesIn[zone]) ? (zonesIn[zone] as unknown[]) : [];
    for (const item of items) {
      if (!isModule(item) || placed.has(item)) continue;
      placed.add(item);
      if (FITS[item].includes(zone)) {
        zones[zone].push(item);
      } else {
        misplaced.push(item);
      }
    }
  }

  const hiddenIn = Array.isArray(input.hidden) ? input.hidden.filter(isModule) : [];
  const hidden = new Set<ModuleId>(hiddenIn);
  // A layout that never mentioned a module gets it as Vitra has it by default, hidden or not.
  for (const module of MODULES) {
    if (!placed.has(module)) {
      zones[homeZone(module)].push(module);
      if (DEFAULT_LAYOUT.hidden.includes(module)) hidden.add(module);
    }
  }
  for (const module of misplaced) {
    zones[homeZone(module)].push(module);
  }

  const widthsIn = (input.widths ?? {}) as Partial<Record<Column, unknown>>;
  const width = (column: Column) => {
    const raw = widthsIn[column];
    return typeof raw === 'number' && Number.isFinite(raw)
      ? clamp(Math.round(raw), COLUMN_WIDTH)
      : DEFAULT_LAYOUT.widths[column];
  };

  const weightsIn = (input.weights ?? {}) as Record<string, unknown>;
  const weights: Layout['weights'] = {};
  for (const module of MODULES) {
    const raw = weightsIn[module];
    if (typeof raw === 'number' && Number.isFinite(raw) && raw !== 1) {
      weights[module] = clamp(raw, WEIGHT);
    }
  }

  return {
    zones,
    hidden: MODULES.filter((module) => hidden.has(module)),
    widths: { left: width('left'), right: width('right') },
    weights,
    sidebar: input.sidebar === 'rail' ? 'rail' : 'expanded',
  };
}

/** The zone a module is in (every module is in one after {@link normalizeLayout}). */
export function zoneOf(layout: Layout, module: ModuleId): Zone {
  return ZONES.find((zone) => layout.zones[zone].includes(module)) ?? homeZone(module);
}

export function isShown(layout: Layout, module: ModuleId): boolean {
  return !layout.hidden.includes(module);
}

/** Shows or hides a module where it is. */
export function setShown(layout: Layout, module: ModuleId, shown: boolean): Layout {
  const hidden = layout.hidden.filter((m) => m !== module);
  if (!shown) hidden.push(module);
  return { ...layout, hidden: MODULES.filter((m) => hidden.includes(m)) };
}

/**
 * Moves a module to `zone` at `index` (in the zone without the module); a zone it doesn't fit
 * leaves the layout as it was. Moving a hidden module shows it: it was dragged somewhere to
 * be seen.
 */
export function moveModule(layout: Layout, module: ModuleId, zone: Zone, index: number): Layout {
  if (!FITS[module].includes(zone)) {
    return layout;
  }
  const zones = Object.fromEntries(
    ZONES.map((z) => [z, layout.zones[z].filter((m) => m !== module)]),
  ) as Zones;
  const target = zones[zone];
  target.splice(Math.min(Math.max(0, index), target.length), 0, module);
  return setShown({ ...layout, zones }, module, true);
}

/** Modules of a zone that are shown, in order. */
export function shownIn(layout: Layout, zone: Zone): ModuleId[] {
  return layout.zones[zone].filter((module) => isShown(layout, module));
}

/** A column width within its limits, in whole pixels. */
export function clampWidth(width: number): number {
  return clamp(Math.round(width), COLUMN_WIDTH);
}

export function setWidth(layout: Layout, column: Column, width: number): Layout {
  return {
    ...layout,
    widths: { ...layout.widths, [column]: clamp(Math.round(width), COLUMN_WIDTH) },
  };
}

/** Sets a module's share of its zone; 1 (the default) isn't stored. */
export function setWeight(layout: Layout, module: ModuleId, weight: number): Layout {
  const weights = { ...layout.weights };
  const value = clamp(weight, WEIGHT);
  if (Math.abs(value - 1) < 0.01) {
    delete weights[module];
  } else {
    weights[module] = value;
  }
  return { ...layout, weights };
}

export function weightOf(layout: Layout, module: ModuleId): number {
  return layout.weights[module] ?? 1;
}

/**
 * Moves a module to `zone`, just before `before` (or at the end of what the zone shows), and
 * shows it: what a drag and drop does. A zone it doesn't fit leaves the layout as it was.
 */
export function placeModule(
  layout: Layout,
  module: ModuleId,
  zone: Zone,
  before: ModuleId | null,
): Layout {
  if (!FITS[module].includes(zone) || module === before) {
    return layout;
  }
  const rest = layout.zones[zone].filter((m) => m !== module);
  const at = before === null ? -1 : rest.indexOf(before);
  // Without a target, after the last module that shows (hidden ones stay behind it).
  const lastShown = rest.reduce((last, m, i) => (isShown(layout, m) ? i : last), -1);
  return moveModule(layout, module, zone, at === -1 ? lastShown + 1 : at);
}
