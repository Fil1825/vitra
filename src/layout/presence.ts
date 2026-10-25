import { ZONES, type ModuleId, type Zone } from './model.ts';

/**
 * Modules come and go with a motion (the sidebar slides out, the queue folds away): a
 * module just hidden stays in its zone, leaving, until its animation ends. Only the columns
 * move this way: the bars change in customize mode alone, where everything is instant.
 */

/** `entering`: just shown, playing its arrival; `leaving`: hidden, playing its exit. */
export type Phase = 'present' | 'entering' | 'leaving';

export interface Presence {
  module: ModuleId;
  phase: Phase;
}

export type LayoutPresence = Record<Zone, Presence[]>;

const ANIMATED: readonly Zone[] = ['left', 'right'];

/** Everything present at once (the first render, Reduce Motion, customize mode). */
export function settledPresence(shown: readonly ModuleId[]): Presence[] {
  return shown.map((module) => ({ module, phase: 'present' }));
}

export function settledLayout(shown: Record<Zone, readonly ModuleId[]>): LayoutPresence {
  return Object.fromEntries(
    ZONES.map((zone) => [zone, settledPresence(shown[zone])]),
  ) as LayoutPresence;
}

/**
 * What a zone renders after `shown` changed: the modules shown (new ones entering, ones
 * coming back while leaving entering again) and those just hidden, leaving, in the places
 * they had.
 */
export function nextPresence(
  previous: readonly Presence[],
  shown: readonly ModuleId[],
): Presence[] {
  const before = new Map(previous.map((p) => [p.module, p.phase]));
  const next: Presence[] = shown.map((module) => {
    const phase = before.get(module);
    return {
      module,
      phase: phase === undefined || phase === 'leaving' ? 'entering' : phase,
    };
  });
  // Hidden ones stay where they were: after the module listed before them, if any.
  previous.forEach((entry, index) => {
    if (shown.includes(entry.module)) {
      return;
    }
    const leaving: Presence = { module: entry.module, phase: 'leaving' };
    const earlier = previous
      .slice(0, index)
      .reverse()
      .find((p) => next.some((n) => n.module === p.module));
    const at = earlier ? next.findIndex((n) => n.module === earlier.module) + 1 : 0;
    next.splice(at, 0, leaving);
  });
  return next;
}

/**
 * Every zone after `shown` changed. A module moved to another zone (a preset) arrives there
 * and doesn't also linger where it was: one module is never on screen twice.
 */
export function nextLayoutPresence(
  previous: LayoutPresence,
  shown: Record<Zone, readonly ModuleId[]>,
): LayoutPresence {
  const everywhere = new Set(ZONES.flatMap((zone) => shown[zone]));
  return Object.fromEntries(
    ZONES.map((zone) => {
      if (!ANIMATED.includes(zone)) {
        return [zone, settledPresence(shown[zone])];
      }
      const next = nextPresence(previous[zone], shown[zone]).filter(
        (entry) => entry.phase !== 'leaving' || !everywhere.has(entry.module),
      );
      return [zone, next];
    }),
  ) as LayoutPresence;
}

/** The module's animation ended: a leaving one goes, an entering one is simply there. */
export function settle(entries: readonly Presence[], module: ModuleId): Presence[] {
  return entries.flatMap((entry) => {
    if (entry.module !== module) return [entry];
    return entry.phase === 'leaving' ? [] : [{ module, phase: 'present' }];
  });
}

/** Every animation done at once (a safety net when an end event never comes). */
export function settleLayout(presence: LayoutPresence): LayoutPresence {
  return Object.fromEntries(
    ZONES.map((zone) => [
      zone,
      presence[zone].flatMap((entry): Presence[] =>
        entry.phase === 'leaving' ? [] : [{ module: entry.module, phase: 'present' }],
      ),
    ]),
  ) as LayoutPresence;
}

/** Whether anything is still arriving or leaving. */
export function isMoving(presence: LayoutPresence): boolean {
  return ZONES.some((zone) => presence[zone].some((entry) => entry.phase !== 'present'));
}

/**
 * How a zone as a whole moves: `opening` when everything in it is arriving (it was
 * empty), `closing` when everything in it is leaving (it will be empty).
 */
export function zoneMotion(entries: readonly Presence[]): 'opening' | 'closing' | undefined {
  if (entries.length === 0) return undefined;
  if (entries.every((e) => e.phase === 'leaving')) return 'closing';
  if (entries.every((e) => e.phase === 'entering')) return 'opening';
  return undefined;
}

/**
 * How the line before `entries[index]` moves: with the module after it, or, before the
 * second module, with a first one coming or going (one line fewer either way).
 */
export function splitPhase(entries: readonly Presence[], index: number): Phase {
  const own = entries[index]?.phase ?? 'present';
  if (own !== 'present') return own;
  return index === 1 ? (entries[0]?.phase ?? 'present') : 'present';
}
