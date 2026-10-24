/**
 * Theme colors are hex with optional alpha (`#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`). The
 * editor shows them as a color (what `<input type="color">` takes) plus an opacity.
 */

const HEX = /^#(?:[0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

export function isHexColor(value: string): boolean {
  return HEX.test(value);
}

/** `#rrggbb` and an opacity 0..1; null for anything that isn't a hex color. */
export function splitColor(value: string): { rgb: string; alpha: number } | null {
  if (!isHexColor(value)) return null;
  let hex = value.slice(1).toLowerCase();
  if (hex.length <= 4) {
    hex = [...hex].map((digit) => digit + digit).join('');
  }
  const alpha = hex.length === 8 ? parseInt(hex.slice(6), 16) / 255 : 1;
  return { rgb: `#${hex.slice(0, 6)}`, alpha: Math.round(alpha * 100) / 100 };
}

/** A color and an opacity back as hex; fully opaque colors keep six digits. */
export function joinColor(rgb: string, alpha: number): string {
  const base = splitColor(rgb)?.rgb ?? '#000000';
  const clamped = Math.min(1, Math.max(0, alpha));
  if (clamped >= 1) return base.toUpperCase();
  const byte = Math.round(clamped * 255)
    .toString(16)
    .padStart(2, '0');
  return `${base}${byte}`.toUpperCase();
}

/** An opacity as a percentage. */
export const percent = (value: number) => `${Math.round(value * 100)}%`;

/**
 * `a` mixed with `share` of `b` (both hex; their opacities are left out), as `#rrggbb`;
 * `a` itself when either isn't a hex color.
 */
export function mixColors(a: string, b: string, share: number): string {
  const from = splitColor(a)?.rgb;
  const to = splitColor(b)?.rgb;
  if (!from || !to) return a;
  const channel = (hex: string, at: number) => parseInt(hex.slice(at, at + 2), 16);
  const t = Math.min(1, Math.max(0, share));
  return `#${[1, 3, 5]
    .map((at) =>
      Math.round(channel(from, at) * (1 - t) + channel(to, at) * t)
        .toString(16)
        .padStart(2, '0'),
    )
    .join('')}`;
}
