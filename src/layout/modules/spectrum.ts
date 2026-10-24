/**
 * The level meter's motion, kept apart from drawing so it can be tested: readings arrive a
 * few times a second (`player:spectrum`), the meter draws sixty times a second, and this
 * eases each bar towards its reading — up quickly, down slowly — with a peak cap that holds
 * a moment and then falls.
 */

/** Bars a meter shows for its width: about one every 12 px, between 8 and 32. */
export function barCount(width: number): number {
  return Math.max(8, Math.min(32, Math.floor(width / 12)));
}

/**
 * The bands as `count` bars. Bands are evenly spaced on a log scale, so bars are too:
 * each bar takes the bands around its place, interpolated.
 */
export function resample(bands: readonly number[], count: number): number[] {
  if (bands.length === 0) return Array.from({ length: count }, () => 0);
  if (bands.length === 1 || count === 1) return Array.from({ length: count }, () => bands[0] ?? 0);
  return Array.from({ length: count }, (_, bar) => {
    const at = (bar / (count - 1)) * (bands.length - 1);
    const below = Math.floor(at);
    const above = Math.min(bands.length - 1, below + 1);
    const share = at - below;
    return (bands[below] ?? 0) * (1 - share) + (bands[above] ?? 0) * share;
  });
}

/** A slow wave across low bars, for pauses and silence: alive, but calm. */
export function idleLevels(count: number, seconds: number): number[] {
  return Array.from({ length: count }, (_, bar) => {
    const x = count === 1 ? 0 : bar / (count - 1);
    const wave = Math.sin(seconds * 1.3 - x * 5.5) * 0.5 + 0.5;
    const swell = Math.sin(seconds * 0.45 + x * 2.2) * 0.5 + 0.5;
    return 0.06 + wave * 0.13 + swell * 0.07;
  });
}

/** How fast bars follow: time constants in seconds, and the caps' hold and fall. */
const ATTACK = 0.045;
const RELEASE = 0.32;
const PEAK_HOLD = 0.4;
const PEAK_GRAVITY = 1.6;

export class Dynamics {
  values: number[] = [];
  peaks: number[] = [];
  private holds: number[] = [];
  private speeds: number[] = [];

  /** Advances by `dt` seconds towards `targets` (one per bar; a new count starts over). */
  step(targets: readonly number[], dt: number): void {
    if (targets.length !== this.values.length) {
      this.values = targets.map(() => 0);
      this.peaks = targets.map(() => 0);
      this.holds = targets.map(() => 0);
      this.speeds = targets.map(() => 0);
    }
    const rise = 1 - Math.exp(-dt / ATTACK);
    const fall = 1 - Math.exp(-dt / RELEASE);
    targets.forEach((target, bar) => {
      const value = this.values[bar] ?? 0;
      const next = value + (target - value) * (target > value ? rise : fall);
      this.values[bar] = next;
      const peak = this.peaks[bar] ?? 0;
      if (next >= peak) {
        this.peaks[bar] = next;
        this.holds[bar] = PEAK_HOLD;
        this.speeds[bar] = 0;
      } else if ((this.holds[bar] ?? 0) > 0) {
        this.holds[bar] = (this.holds[bar] ?? 0) - dt;
      } else {
        const speed = (this.speeds[bar] ?? 0) + PEAK_GRAVITY * dt;
        this.speeds[bar] = speed;
        this.peaks[bar] = Math.max(next, peak - speed * dt);
      }
    });
  }

  /** Whether anything still moves towards `targets` (else the meter can stop drawing). */
  settled(targets: readonly number[]): boolean {
    return (
      targets.length === this.values.length &&
      targets.every(
        (target, bar) =>
          Math.abs((this.values[bar] ?? 0) - target) < 0.002 &&
          Math.abs((this.peaks[bar] ?? 0) - (this.values[bar] ?? 0)) < 0.002,
      )
    );
  }
}
