import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  PointerSensor,
  closestCenter,
  pointerWithin,
  useDndContext,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type DragStartEvent,
} from '@dnd-kit/core';
import {
  SortableContext,
  horizontalListSortingStrategy,
  sortableKeyboardCoordinates,
  useSortable,
  verticalListSortingStrategy,
} from '@dnd-kit/sortable';
import { CSS } from '@dnd-kit/utilities';
import { EyeOff, GripVertical, MoveRight } from 'lucide-react';
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import { create } from 'zustand';

import { ControlBar } from '../app/ControlBar.tsx';
import { Sidebar } from '../app/Sidebar.tsx';
import { TitleBar } from '../app/TitleBar.tsx';
import { IconButton } from '../components/ui/IconButton.tsx';
import { LibraryView } from '../features/library/LibraryView.tsx';
import { LyricsPanel } from '../features/lyrics/LyricsPanel.tsx';
import { QueuePanel } from '../features/queue/QueuePanel.tsx';
import { showContextMenu } from '../lib/menu.ts';
import { routeKey } from '../lib/scrollMemory.ts';
import { useScrollMemory } from '../lib/useScrollMemory.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useEnvironment } from '../stores/environment.ts';
import { useNavigation } from '../stores/navigation.ts';
import { useSettings } from '../stores/settings.ts';
import {
  FITS,
  ZONES,
  clampWidth,
  placeModule,
  setWeight,
  setWidth,
  shownIn,
  weightOf,
  zoneOf,
  type Column,
  type Layout,
  type ModuleId,
  type Zone,
} from './model.ts';
import { ArtworkPanel } from './modules/ArtworkPanel.tsx';
import { LevelMeter } from './modules/LevelMeter.tsx';
import { CustomizePalette, ModuleChip } from './Palette.tsx';
import {
  isMoving,
  nextLayoutPresence,
  settle,
  settledLayout,
  settleLayout,
  splitPhase,
  zoneMotion,
  type LayoutPresence,
  type Phase,
} from './presence.ts';
import { setModuleShown, updateLayout, useCustomizing, useLayout } from './state.ts';
import styles from './Workspace.module.css';

/** The module being dragged, so zones it doesn't fit can say so. */
const useDragging = create<{ module: ModuleId | null }>(() => ({ module: null }));

const PALETTE_PREFIX = 'palette:';
const ZONE_PREFIX = 'zone:';

const isColumn = (zone: Zone): zone is Column => zone === 'left' || zone === 'right';

/**
 * What a module is dropped on: the module under the pointer, else the zone under it. The
 * zones differ too much in size for the nearest centre to mean anything (a small level meter
 * would win over the column the pointer is in). The keyboard has no pointer: nearest centre.
 */
const dropTarget: CollisionDetection = (args) => {
  if (!args.pointerCoordinates) return closestCenter(args);
  const hits = pointerWithin(args);
  const module = hits.find((hit) => !String(hit.id).startsWith(ZONE_PREFIX));
  return module ? [module] : hits.slice(0, 1);
};

/** The zone a drop target is or is in. */
function zoneOfTarget(layout: Layout, id: string | number | undefined): Zone | null {
  if (id === undefined) return null;
  const target = String(id);
  return target.startsWith(ZONE_PREFIX)
    ? (target.slice(ZONE_PREFIX.length) as Zone)
    : zoneOf(layout, target as ModuleId);
}

/**
 * The main window's layout: the top bar, the columns with the main view between them, and
 * the bottom bar, each holding the modules the layout puts there. Columns resize at their
 * inner edge and modules in a column at the line between them; in customize mode modules
 * drag between zones (dnd-kit), hide, and come back from the palette.
 */
export function Workspace({ onOpenSettings }: { onOpenSettings(): void }) {
  const layout = useLayout();
  const customizing = useCustomizing((s) => s.on);
  const lyricsVisible = useSettings((s) => s.settings.lyrics.visible);
  const mac = useEnvironment((s) => s.platform === 'macos');
  const route = useNavigation((s) => s.route);
  const dragging = useDragging((s) => s.module !== null);

  // Hidden lyrics take their panel along (`lyrics.visible`), wherever it is.
  const visible = (zone: Zone) =>
    shownIn(layout, zone).filter((module) => module !== 'lyrics' || lyricsVisible);
  const shown = Object.fromEntries(ZONES.map((zone) => [zone, visible(zone)])) as Record<
    Zone,
    ModuleId[]
  >;
  const presence = usePresence(shown, !customizing);

  // macOS draws the traffic lights at the window's top-left corner: whatever is there makes
  // room for them (the sidebar has its own room).
  const lights = !mac
    ? undefined
    : shown.top.length > 0 || customizing
      ? 'top'
      : shown.left.length > 0
        ? shown.left[0] === 'sidebar'
          ? undefined
          : 'left'
        : 'header';

  return (
    <LayoutDnd layout={layout}>
      <div
        className={styles.shell}
        data-customizing={customizing || undefined}
        data-dragging={dragging || undefined}
        data-lights={lights}
      >
        <ZoneView zone="top" presence={presence} layout={layout} />
        <div className={styles.middle}>
          <ZoneView zone="left" presence={presence} layout={layout} />
          <div className={styles.center} data-part="center">
            <TitleBar className={styles.header} onOpenSettings={onOpenSettings} />
            {/* Keyed by route: each screen comes back where it was left (MainArea). */}
            <MainArea key={JSON.stringify(route)} scrollKey={routeKey(route)} />
          </div>
          <ZoneView zone="right" presence={presence} layout={layout} />
        </div>
        <ZoneView zone="bottom" presence={presence} layout={layout} />
      </div>
      {customizing && <CustomizePalette layout={layout} />}
    </LayoutDnd>
  );
}

interface PresenceState {
  zones: LayoutPresence;
  /** A module's arrival or exit ended. */
  settled(zone: Zone, module: ModuleId): void;
}

/**
 * What each zone renders: the modules shown, plus those just hidden until they have left
 * (presence.ts). Without motion, or in customize mode, changes are instant.
 */
function usePresence(shown: Record<Zone, ModuleId[]>, animate: boolean): PresenceState {
  const motion = useAppearance((s) => s.effects.motion);
  const duration = useAppearance((s) => s.theme.motion.duration);
  const key = JSON.stringify(shown);
  const [zones, setZones] = useState(() => settledLayout(shown));
  const [lastKey, setLastKey] = useState(key);
  if (key !== lastKey) {
    setLastKey(key);
    setZones(motion && animate ? nextLayoutPresence(zones, shown) : settledLayout(shown));
  }

  // A safety net for an end event that never comes (e.g. a theme's CSS stopped the motion).
  const moving = isMoving(zones);
  useEffect(() => {
    if (!moving) return;
    const timer = window.setTimeout(() => setZones(settleLayout), duration + 250);
    return () => window.clearTimeout(timer);
  }, [moving, zones, duration]);

  const settled = useCallback(
    (zone: Zone, module: ModuleId) =>
      setZones((current) => ({ ...current, [zone]: settle(current[zone], module) })),
    [],
  );
  return { zones, settled };
}

/** True for a moment (`ms`) after `value` changes, not when it first renders. */
function useJustChanged(value: unknown, ms: number): boolean {
  const [last, setLast] = useState(value);
  const [changed, setChanged] = useState(false);
  if (value !== last) {
    setLast(value);
    setChanged(true);
  }
  useEffect(() => {
    if (!changed) return;
    const timer = window.setTimeout(() => setChanged(false), ms);
    return () => window.clearTimeout(timer);
  }, [changed, last, ms]);
  return changed;
}

/** The main view of one screen, scrolled where it was left. */
function MainArea({ scrollKey }: { scrollKey: string }) {
  const ref = useRef<HTMLElement>(null);
  useScrollMemory(ref, scrollKey);
  return (
    <main
      ref={ref}
      className={styles.main}
      data-part="main"
      data-scroll-container
      data-scroll-key={scrollKey}
    >
      <LibraryView />
    </main>
  );
}

/** Drag and drop for customize mode; outside it nothing is draggable. */
function LayoutDnd({ layout, children }: { layout: Layout; children: ReactNode }) {
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor, { coordinateGetter: sortableKeyboardCoordinates }),
  );
  const dragging = useDragging((s) => s.module);

  const moduleOf = (id: string | number) => String(id).replace(PALETTE_PREFIX, '') as ModuleId;

  const onDragStart = ({ active }: DragStartEvent) =>
    useDragging.setState({ module: moduleOf(active.id) });

  const onDragEnd = ({ active, over }: DragEndEvent) => {
    useDragging.setState({ module: null });
    if (!over) return;
    const module = moduleOf(active.id);
    const target = String(over.id);
    const zone = zoneOfTarget(layout, target);
    if (!zone) return;
    if (target.startsWith(ZONE_PREFIX)) {
      updateLayout((l) => placeModule(l, module, zone, null));
      return;
    }
    const overModule = target as ModuleId;
    const list = shownIn(layout, zone);
    const from = list.indexOf(module);
    const to = list.indexOf(overModule);
    // Within one zone, dragging forward lands after the module dropped on, as the sortable
    // list showed while dragging. From elsewhere, the half of it dropped on decides.
    const dragged = active.rect.current.translated;
    const column = isColumn(zone);
    const after =
      from !== -1
        ? from < to
        : dragged !== null &&
          (column
            ? dragged.top + dragged.height / 2 > over.rect.top + over.rect.height / 2
            : dragged.left + dragged.width / 2 > over.rect.left + over.rect.width / 2);
    const before = after ? (list[to + 1] ?? null) : overModule;
    updateLayout((l) => placeModule(l, module, zone, before));
  };

  return (
    <DndContext
      sensors={sensors}
      collisionDetection={dropTarget}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragCancel={() => useDragging.setState({ module: null })}
    >
      {children}
      <DragOverlay dropAnimation={null}>
        {dragging && <ModuleChip module={dragging} lifted />}
      </DragOverlay>
    </DndContext>
  );
}

function ZoneView({
  zone,
  presence,
  layout,
}: {
  zone: Zone;
  presence: PresenceState;
  layout: Layout;
}) {
  const { t } = useTranslation();
  const customizing = useCustomizing((s) => s.on);
  const dragging = useDragging((s) => s.module);
  const duration = useAppearance((s) => s.theme.motion.duration);
  const { setNodeRef } = useDroppable({ id: `${ZONE_PREFIX}${zone}`, disabled: !customizing });
  // Over the zone itself or over a module in it.
  const isOver = zoneOfTarget(layout, useDndContext().over?.id) === zone;
  // A column's width while its edge is dragged; saved when the drag ends.
  const [width, setLiveWidth] = useState<number | null>(null);
  const entries = presence.zones[zone];
  const column = isColumn(zone);
  const rail =
    column && entries.length === 1 && entries[0]?.module === 'sidebar' && layout.sidebar === 'rail';
  // Between the full sidebar and the rail the column's width moves (a transition, only
  // then: a column following a dragged edge or the window's width mustn't lag behind).
  const morphing = useJustChanged(rail, duration + 50);

  if (entries.length === 0 && !customizing) {
    return null;
  }
  const modules = entries.map((entry) => entry.module);
  const motion = column ? zoneMotion(entries) : undefined;
  const style = column
    ? ({ '--_width': `${width ?? layout.widths[zone]}px` } as CSSProperties)
    : undefined;
  const refuses = dragging !== null && !FITS[dragging].includes(zone);

  return (
    <div
      ref={setNodeRef}
      className={column ? styles.column : styles.bar}
      data-zone={zone}
      data-empty={entries.length === 0 || undefined}
      data-rail={rail || undefined}
      data-presence={motion}
      data-morphing={(morphing && width === null) || undefined}
      data-over={(customizing && isOver && !refuses) || undefined}
      data-refuses={refuses || undefined}
      style={style}
    >
      <SortableContext
        items={modules}
        strategy={column ? verticalListSortingStrategy : horizontalListSortingStrategy}
      >
        {entries.map(({ module, phase }, index) => (
          <Fragment key={module}>
            {column && index > 0 && (
              <SplitHandle
                above={modules[index - 1] as ModuleId}
                below={module}
                layout={layout}
                phase={splitPhase(entries, index)}
              />
            )}
            <ModuleFrame
              module={module}
              zone={zone}
              layout={layout}
              rail={rail}
              phase={phase}
              onSettled={presence.settled}
            />
          </Fragment>
        ))}
      </SortableContext>
      {customizing && entries.length === 0 && (
        <div className={styles.dropHint}>{t(`layout.zones.${zone}`)}</div>
      )}
      {column && !rail && motion === undefined && entries.length > 0 && (
        <ColumnResizer column={zone} width={layout.widths[zone]} onLive={setLiveWidth} />
      )}
    </div>
  );
}

function ModuleFrame({
  module,
  zone,
  layout,
  rail,
  phase,
  onSettled,
}: {
  module: ModuleId;
  zone: Zone;
  layout: Layout;
  rail: boolean;
  phase: Phase;
  onSettled(zone: Zone, module: ModuleId): void;
}) {
  const { t } = useTranslation();
  const customizing = useCustomizing((s) => s.on);
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: module,
    disabled: !customizing,
  });
  const element = useRef<HTMLDivElement | null>(null);
  const setRef = useCallback(
    (node: HTMLDivElement | null) => {
      element.current = node;
      setNodeRef(node);
    },
    [setNodeRef],
  );
  // A module on its way out takes no focus or clicks; focus in it goes back to the page.
  useLayoutEffect(() => {
    const focused = document.activeElement;
    if (
      phase === 'leaving' &&
      focused instanceof HTMLElement &&
      element.current?.contains(focused)
    ) {
      focused.blur();
    }
  }, [phase]);
  const style: CSSProperties = {
    flex: isColumn(zone) ? `${weightOf(layout, module)} 1 0` : undefined,
    transform: CSS.Translate.toString(transform),
    transition,
  };

  const moveMenu = () =>
    showContextMenu([
      ...FITS[module]
        .filter((target) => target !== zone)
        .map((target) => ({
          label: t(`layout.moveTo.${target}`),
          action: () => updateLayout((l) => placeModule(l, module, target, null)),
        })),
      'separator',
      { label: t('layout.hide'), action: () => setModuleShown(module, false) },
    ]).catch((err: unknown) => console.error('The module menu failed', err));

  return (
    <div
      ref={setRef}
      className={styles.module}
      data-module={module}
      data-part={module}
      data-dragging={isDragging || undefined}
      data-phase={phase === 'present' ? undefined : phase}
      inert={phase === 'leaving'}
      style={style}
      onAnimationEnd={(event) => {
        if (phase !== 'present' && event.target === event.currentTarget) {
          onSettled(zone, module);
        }
      }}
    >
      <ModuleContent module={module} rail={rail} />
      {customizing && (
        <div className={styles.frame} {...attributes} {...listeners}>
          <span className={styles.frameLabel}>
            <GripVertical size={14} aria-hidden="true" />
            <span className={styles.frameName}>{t(`layout.modules.${module}`)}</span>
          </span>
          <span className={styles.frameButtons}>
            <IconButton
              label={t('layout.move')}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={moveMenu}
            >
              <MoveRight size={15} />
            </IconButton>
            <IconButton
              label={t('layout.hide')}
              onPointerDown={(event) => event.stopPropagation()}
              onClick={() => setModuleShown(module, false)}
            >
              <EyeOff size={15} />
            </IconButton>
          </span>
        </div>
      )}
    </div>
  );
}

function ModuleContent({ module, rail }: { module: ModuleId; rail: boolean }) {
  switch (module) {
    case 'sidebar':
      return <Sidebar className={styles.fill} mode={rail ? 'rail' : 'expanded'} />;
    case 'queue':
      return <QueuePanel className={styles.fill} />;
    case 'lyrics':
      return <LyricsPanel className={styles.fill} />;
    case 'artwork':
      return <ArtworkPanel className={styles.fill} />;
    case 'controls':
      return <ControlBar className={styles.fill} />;
    case 'level':
      return <LevelMeter className={styles.fill} />;
  }
}

/** Follows a pointer drag from `event`: `onMove` gets the distance moved, `onEnd` too. */
function dragFrom(
  event: ReactPointerEvent<HTMLElement>,
  onMove: (dx: number, dy: number) => void,
  onEnd: (dx: number, dy: number) => void,
): void {
  if (event.button !== 0) return;
  event.preventDefault();
  const handle = event.currentTarget;
  const start = { x: event.clientX, y: event.clientY };
  handle.setPointerCapture(event.pointerId);
  const move = (e: PointerEvent) => onMove(e.clientX - start.x, e.clientY - start.y);
  const up = (e: PointerEvent) => {
    handle.removeEventListener('pointermove', move);
    handle.removeEventListener('pointerup', up);
    handle.removeEventListener('pointercancel', up);
    onEnd(e.clientX - start.x, e.clientY - start.y);
  };
  handle.addEventListener('pointermove', move);
  handle.addEventListener('pointerup', up);
  handle.addEventListener('pointercancel', up);
}

/** The inner edge of a column, dragged to change its width (saved when let go). */
function ColumnResizer({
  column,
  width,
  onLive,
}: {
  column: Column;
  width: number;
  onLive(width: number | null): void;
}) {
  const { t } = useTranslation();
  // The left column grows to the right, the right one to the left.
  const sign = column === 'left' ? 1 : -1;
  return (
    <div
      className={styles.resizer}
      data-column={column}
      role="separator"
      aria-orientation="vertical"
      aria-label={t('layout.resize')}
      onPointerDown={(event) =>
        dragFrom(
          event,
          (dx) => onLive(clampWidth(width + sign * dx)),
          (dx) => {
            onLive(null);
            if (dx !== 0) updateLayout((l) => setWidth(l, column, width + sign * dx));
          },
        )
      }
      onDoubleClick={() => updateLayout((l) => setWidth(l, column, column === 'left' ? 232 : 320))}
    />
  );
}

/**
 * The line between two modules in a column: dragging it moves height from one to the other
 * (their shares of the column change, the total stays).
 */
function SplitHandle({
  above,
  below,
  layout,
  phase,
}: {
  above: ModuleId;
  below: ModuleId;
  layout: Layout;
  /** Coming or going with a module next to it (one line fewer either way). */
  phase: Phase;
}) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDivElement>(null);
  const shares = (dy: number) => {
    const upper = ref.current?.previousElementSibling as HTMLElement | null;
    const lower = ref.current?.nextElementSibling as HTMLElement | null;
    if (!upper || !lower) return null;
    const a = upper.getBoundingClientRect().height;
    const b = lower.getBoundingClientRect().height;
    const total = weightOf(layout, above) + weightOf(layout, below);
    const height = Math.max(48, Math.min(a + b - 48, a + dy));
    const upperShare = (total * height) / (a + b);
    return { upper, lower, upperShare, lowerShare: total - upperShare };
  };
  return (
    <div
      ref={ref}
      className={styles.split}
      data-phase={phase === 'present' ? undefined : phase}
      role="separator"
      aria-orientation="horizontal"
      aria-label={t('layout.resize')}
      onPointerDown={(event) => {
        const begin = shares(0);
        if (!begin) return;
        dragFrom(
          event,
          (_, dy) => {
            const next = shares(dy);
            if (!next) return;
            // Shown at once by setting the flex on the elements; saved when let go.
            next.upper.style.flex = `${next.upperShare} 1 0`;
            next.lower.style.flex = `${next.lowerShare} 1 0`;
          },
          (_, dy) => {
            const next = shares(dy);
            if (!next || dy === 0) return;
            updateLayout((l) =>
              setWeight(setWeight(l, above, next.upperShare), below, next.lowerShare),
            );
          },
        );
      }}
      onDoubleClick={() => updateLayout((l) => setWeight(setWeight(l, above, 1), below, 1))}
    />
  );
}
