import { EASINGS, type Theme } from './schema.ts';

const px = (value: number) => `${value}px`;

/** Spacing steps exposed as `--vitra-space-N` (N × theme spacing unit). */
export const SPACE_STEPS = [1, 2, 3, 4, 5, 6, 8, 10, 12] as const;

/** Corner radius of covers per artwork shape (a share of the cover, so every size matches). */
export const ARTWORK_RADIUS = { square: 'initial', rounded: '22%', circle: '50%' } as const;

/** Number of color stops the backdrop uses; shorter gradients repeat their colors. */
export const BACKDROP_COLORS = 3;

/** Seconds for one cycle of the background flow at speed 1. */
const BACKDROP_CYCLE_S = 48;

/**
 * Resolves a theme into the CSS custom properties every component reads.
 * Components never hard-code colors, radii or blur: they use these variables.
 */
export function themeToCssVars(theme: Theme): Record<string, string> {
  const { colors, glass, typography, radius, spacing, background, motion } = theme;
  const vars: Record<string, string> = {
    '--vitra-color-background': colors.background,
    '--vitra-color-text': colors.text,
    '--vitra-color-text-secondary': colors.textSecondary,
    '--vitra-color-text-tertiary': colors.textTertiary,
    '--vitra-color-accent': colors.accent,
    '--vitra-color-accent-contrast': colors.accentContrast,
    '--vitra-color-separator': colors.separator,
    '--vitra-color-danger': colors.danger,

    '--vitra-glass-blur': px(glass.blur),
    '--vitra-glass-saturation': String(glass.saturation),
    '--vitra-glass-tint': glass.tint,
    '--vitra-glass-opacity': String(glass.opacity),
    '--vitra-glass-border': glass.border,
    '--vitra-glass-border-width': px(glass.borderWidth),
    '--vitra-glass-highlight': String(glass.highlight),
    '--vitra-glass-highlight-color': glass.highlightColor,
    '--vitra-glass-refraction': String(glass.refraction),
    '--vitra-glass-shadow': `0 ${px(Math.round(glass.shadowBlur / 4))} ${px(glass.shadowBlur)} ${glass.shadow}`,

    '--vitra-font-family': typography.fontFamily,
    '--vitra-font-size': px(typography.fontSize),

    '--vitra-radius-sm': px(radius.small),
    '--vitra-radius-md': px(radius.medium),
    '--vitra-radius-lg': px(radius.large),

    '--vitra-backdrop-base-opacity': String(background.baseOpacity),
    '--vitra-backdrop-opacity': String(background.gradientOpacity),
    '--vitra-backdrop-artwork-opacity': String(background.artworkOpacity),
    '--vitra-backdrop-duration':
      motion.backgroundSpeed > 0
        ? `${Math.round(BACKDROP_CYCLE_S / motion.backgroundSpeed)}s`
        : '0s',

    '--vitra-motion-duration': `${motion.duration}ms`,
    '--vitra-motion-easing': EASINGS[motion.easing],

    // `initial` leaves each place its own fallback (square covers keep their corners, the
    // placeholder its soft text color), and clears the last theme's value.
    '--vitra-artwork-radius': ARTWORK_RADIUS[theme.artwork?.shape ?? 'square'],
    '--vitra-artwork-placeholder': theme.artwork?.placeholderColor ?? 'initial',
  };

  for (const step of SPACE_STEPS) {
    vars[`--vitra-space-${step}`] = px(step * spacing.unit);
  }
  for (const [i, color] of backdropColors(background.gradient).entries()) {
    vars[`--vitra-backdrop-${i + 1}`] = color;
  }
  return vars;
}

/** Picks exactly {@link BACKDROP_COLORS} colors, repeating short gradients. */
export function backdropColors(gradient: readonly string[]): string[] {
  return Array.from({ length: BACKDROP_COLORS }, (_, i) => gradient[i % gradient.length] ?? '');
}
