import { LP_SPEED, VINYL_SPEED_MAX, VINYL_SPEED_MIN } from '../../lib/settings.ts';

/**
 * The record speed slider (pure). A record player's classic speeds are detents: a stretch
 * of the slider's travel around each holds that speed exactly, so the thumb sticks to it a
 * little on the way past, and arrow keys still move on (a few presses later) instead of
 * being pulled back.
 */

/** 33⅓, 45 and 78 turns a minute. */
export const DETENTS: readonly number[] = [LP_SPEED, 45, 78];
/** Slider travel (in turns a minute) a detent holds on either side. */
const HOLD = 1.5;

/** The slider's range: the speeds plus the stretch every detent holds. */
export const DIAL_MIN = VINYL_SPEED_MIN;
export const DIAL_MAX = VINYL_SPEED_MAX + 2 * HOLD * DETENTS.length;
/** One arrow key press. */
export const DIAL_STEP = 0.5;

const isDetent = (speed: number, detent: number) => Math.abs(speed - detent) < 0.01;

/** The speed at a place on the slider. */
export function speedAt(position: number): number {
  let shift = 0;
  for (const detent of DETENTS) {
    const start = detent + shift;
    if (position < start) break;
    if (position <= start + 2 * HOLD) return detent;
    shift += 2 * HOLD;
  }
  return Math.min(VINYL_SPEED_MAX, Math.max(VINYL_SPEED_MIN, position - shift));
}

/** Where a speed is on the slider (a detent: the middle of its stretch). */
export function positionOf(speed: number): number {
  let shift = 0;
  for (const detent of DETENTS) {
    if (isDetent(speed, detent)) return detent + shift + HOLD;
    if (speed < detent) break;
    shift += 2 * HOLD;
  }
  return speed + shift;
}

/** A speed as people write it: "33⅓", "45", "52.5" (in the language's own digits). */
export function formatSpeed(speed: number, language: string): string {
  if (isDetent(speed, LP_SPEED)) return '33⅓';
  return new Intl.NumberFormat(language, { maximumFractionDigits: 1 }).format(speed);
}
