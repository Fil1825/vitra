/**
 * A turntable's platter: how a record's speed changes on the way to the speed it should
 * have (pure; the spin clock in spin.ts runs it every frame).
 */

/** Seconds to reach full speed from a standstill, and to come to a stop: the same at any speed. */
export const SPIN_UP_S = 0.8;
export const SPIN_DOWN_S = 1.4;

/** Degrees a second at `rpm` turns a minute. */
export function degreesPerSecond(rpm: number): number {
  return rpm * 6;
}

/**
 * The platter's speed `dt` seconds on, heading for `goal` while the record's own speed is
 * `full` (all in degrees a second). The pull scales with `full`, so a 78 gets going and
 * stops in the time a 33⅓ does; a platter above `full` (the speed was just lowered) eases
 * down from its own speed. Never overshoots `goal`.
 */
export function nextVelocity(velocity: number, goal: number, full: number, dt: number): number {
  if (goal > velocity) {
    return Math.min(goal, velocity + (full / SPIN_UP_S) * dt);
  }
  return Math.max(goal, velocity - (Math.max(full, velocity) / SPIN_DOWN_S) * dt);
}
