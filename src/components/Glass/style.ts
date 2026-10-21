import type { CSSProperties } from 'react';

type RadiusToken = 'sm' | 'md' | 'lg';

export interface GlassOptions {
  /** Backdrop blur in px. */
  blur?: number;
  /** Backdrop saturation multiplier. */
  saturation?: number;
  /** Tint color laid over the blurred backdrop. */
  tint?: string;
  /** Tint opacity, 0–1. */
  opacity?: number;
  /** Corner radius in px or a theme radius token. */
  radius?: number | RadiusToken;
  /** `false` hides the border; a color replaces the theme's border color. */
  border?: boolean | string;
  /** Specular rim highlight strength, 0–1. */
  highlight?: number;
  /** Edge refraction strength, 0–1; 0 disables it. Rendered only where the engine supports it. */
  refraction?: number;
}

/**
 * Inline custom properties for the options that override theme defaults.
 * Anything left undefined falls back to the `--vitra-glass-*` theme variables in CSS.
 */
export function glassStyle(options: GlassOptions): CSSProperties {
  const vars: Record<string, string> = {};
  if (options.blur !== undefined) vars['--glass-blur'] = `${options.blur}px`;
  if (options.saturation !== undefined) vars['--glass-saturation'] = String(options.saturation);
  if (options.tint !== undefined) vars['--glass-tint'] = options.tint;
  if (options.opacity !== undefined) vars['--glass-opacity'] = String(options.opacity);
  if (options.highlight !== undefined) vars['--glass-highlight'] = String(options.highlight);
  if (typeof options.radius === 'number') vars['--glass-radius'] = `${options.radius}px`;
  if (typeof options.radius === 'string')
    vars['--glass-radius'] = `var(--vitra-radius-${options.radius})`;
  if (options.border === false) vars['--glass-border'] = 'transparent';
  if (typeof options.border === 'string') vars['--glass-border'] = options.border;
  return vars as CSSProperties;
}
