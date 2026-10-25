import { useAppearance } from '../../stores/appearance.ts';
import { usePlayer } from '../../stores/player.ts';
import { useSettings } from '../../stores/settings.ts';
import { vinylSpeed } from '../../theme/artwork.ts';
import { degreesPerSecond, nextVelocity } from './platter.ts';

/**
 * Covers turning like records: one shared angle for every playing cover in the window
 * (they are the same record), at the chosen speed (33⅓ turns a minute unless the user or
 * the theme says otherwise) while music plays. Speeding up and slowing down take a moment,
 * like a turntable's platter, also when the speed changes; a pause stops the record where
 * it is, so it carries on from there. Only `transform` changes, so nothing is laid out again.
 */

const discs = new Set<HTMLElement>();
let angle = 0;
let velocity = 0;
let frame = 0;
let last = 0;
/** A speed being tried out (the slider in Settings while it is dragged). */
let preview: number | null = null;

/** The record's speed in degrees a second, read every frame so changes apply at once. */
function fullSpeed(): number {
  const setting = useSettings.getState().settings.appearance.vinylSpeed;
  return degreesPerSecond(preview ?? vinylSpeed(useAppearance.getState().theme, setting));
}

function tick(now: number): void {
  const dt = Math.min(0.1, (now - last) / 1000);
  last = now;
  const full = fullSpeed();
  const target = usePlayer.getState().state.status === 'playing' ? full : 0;
  velocity = nextVelocity(velocity, target, full, dt);
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

/** Turns the records at `rpm` until called with `null` (then the setting's speed again). */
export function previewVinylSpeed(rpm: number | null): void {
  preview = rpm;
}

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
