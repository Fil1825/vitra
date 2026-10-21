/** `m:ss`, or `h:mm:ss` from an hour on. Negative and invalid values read as 0. */
export function formatTime(seconds: number): string {
  const total = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = String(total % 60).padStart(2, '0');
  return h > 0 ? `${h}:${String(m).padStart(2, '0')}:${s}` : `${m}:${s}`;
}

/** Playback speed as shown in the UI: `1×`, `1.25×`, `0.5×`. */
export function formatSpeed(speed: number): string {
  return `${Number(speed.toFixed(2))}×`;
}

/** A total length for list headers: `42 min`, `1 h 05 min` (anything playable is at least 1 min). */
export function formatTotal(seconds: number, units: { hours: string; minutes: string }): string {
  const minutes = seconds > 0 ? Math.max(1, Math.round(seconds / 60)) : 0;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h > 0
    ? `${h} ${units.hours} ${String(m).padStart(2, '0')} ${units.minutes}`
    : `${m} ${units.minutes}`;
}
