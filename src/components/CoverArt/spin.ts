import { usePlayer } from '../../stores/player.ts';

/**
 * Covers turning like records: one shared angle for every playing cover in the window
 * (they are the same record), at 33⅓ turns a minute while music plays. Speeding up and
 * slowing down take a moment, like a turntable's platter; a pause stops the record where
 * it is, so it carries on from there. Only `transform` changes, so nothing is laid out again.
 */

/** 33⅓ turns a minute, in degrees per second. */
const SPEED = (360 * 100) / 3 / 60;
/** Seconds to reach full speed, and to come to a stop. */
const SPIN_UP_S = 0.8;
const SPIN_DOWN_S = 1.4;

const discs = new Set<HTMLElement>();
let angle = 0;
let velocity = 0;
let frame = 0;
let last = 0;

const goal = () => (usePlayer.getState().state.status === 'playing' ? SPEED : 0);

function tick(now: number): void {
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  const target = goal();
  const rate = (target > velocity ? SPEED / SPIN_UP_S : SPEED / SPIN_DOWN_S) * dt;
  velocity =
    target > velocity ? Math.min(target, velocity + rate) : Math.max(target, velocity - rate);
  angle = (angle + velocity * dt) % 360;
  const transform = `rotate(${angle.toFixed(2)}deg)`;
  for (const disc of discs) disc.style.transform = transform;
  frame = velocity > 0 || target > 0 ? requestAnimationFrame(tick) : 0;
}

function wake(): void {
  if (frame === 0 && discs.size > 0) {
    last = performance.now();
    frame = requestAnimationFrame(tick);
  }
}

usePlayer.subscribe((state, previous) => {
  if (state.state.status !== previous.state.status) wake();
});

/** Turns `disc` with the others from now on; the returned function stops it. */
export function spin(disc: HTMLElement): () => void {
  discs.add(disc);
  disc.style.transform = `rotate(${angle.toFixed(2)}deg)`;
  wake();
  return () => {
    discs.delete(disc);
    disc.style.transform = '';
    if (discs.size === 0 && frame !== 0) {
      cancelAnimationFrame(frame);
      frame = 0;
      velocity = 0;
    }
  };
}
