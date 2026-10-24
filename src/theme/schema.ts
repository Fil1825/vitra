import { z } from 'zod';

import { layoutSchema } from '../layout/model.ts';

// A theme is plain data: it is turned into CSS custom properties and never executed.
// This schema is the single source for built-in themes and imported `.vitratheme` files;
// docs/theme.schema.json is generated from it (`npm run theme-schema`).

const hexColor = z
  .string()
  .regex(
    /^#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/,
    'Expected a hex color such as #3FA9F5 or #3FA9F580',
  );

const fraction = z.number().min(0).max(1);
const radius = z.number().min(0).max(48);

// Font stacks end up inside a CSS declaration, so anything that could end it (`;`, `}`) or
// pull in a resource (`url(`) is rejected by allowing only names, quotes and separators.
const fontStack = z
  .string()
  .min(1)
  .max(300)
  .regex(/^[\w\s,'"-]+$/, 'Only font names, quotes, commas, spaces and hyphens are allowed');

/** Image files a theme may carry in its `assets/` folder (no SVG: it can hold scripts). */
export const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp', 'gif'] as const;
/** Font files a theme may carry. */
export const FONT_EXTENSIONS = ['woff2', 'woff', 'ttf', 'otf'] as const;

const assetPath = (extensions: readonly string[]) =>
  z.string().regex(
    // No flags: the pattern goes into the JSON Schema as it is.
    new RegExp(`^assets/[A-Za-z0-9][A-Za-z0-9._-]{0,63}\\.(?:${extensions.join('|')})$`),
    `A file in the theme's assets folder: assets/<name>.${extensions.join('|')}`,
  );

/** Named easings themes can pick from; free-form timing functions are not accepted. */
export const EASINGS = {
  standard: 'cubic-bezier(0.2, 0, 0, 1)',
  emphasized: 'cubic-bezier(0.3, 0, 0, 1.15)',
  linear: 'linear',
} as const;

const easingName = z.enum(['standard', 'emphasized', 'linear']);

export const themeSchema = z.object({
  /** Where an editor finds this schema (docs/theme.schema.json); Vitra ignores it. */
  $schema: z.string().max(300).optional(),
  schemaVersion: z.literal(1),
  id: z.string().regex(/^[a-z0-9-]{1,64}$/, 'Lowercase letters, digits and hyphens only'),
  name: z.string().min(1).max(64),
  author: z.string().max(100).optional(),
  /** Light or dark: drives the system appearance (window chrome, vibrancy, scrollbars). */
  appearance: z.enum(['light', 'dark']),
  colors: z.object({
    /** Window base painted over the system vibrancy. */
    background: hexColor,
    text: hexColor,
    textSecondary: hexColor,
    textTertiary: hexColor,
    accent: hexColor,
    /** Text and icons drawn on top of the accent color. */
    accentContrast: hexColor,
    separator: hexColor,
    danger: hexColor,
  }),
  glass: z.object({
    /** Backdrop blur in px. */
    blur: z.number().min(0).max(80),
    /** Backdrop saturation multiplier. */
    saturation: z.number().min(0).max(3),
    tint: hexColor,
    /** Opacity of the tint over the blurred backdrop. */
    opacity: fraction,
    border: hexColor,
    borderWidth: z.number().min(0).max(4),
    /** Strength of the specular rim highlight. */
    highlight: fraction,
    highlightColor: hexColor,
    /** Edge distortion strength; only rendered where the engine supports it. */
    refraction: fraction,
    shadow: hexColor,
    shadowBlur: z.number().min(0).max(120),
  }),
  typography: z.object({
    fontFamily: fontStack,
    fontSize: z.number().min(11).max(20),
    /** Font files the theme brings; `fontFamily` names them like any installed font. */
    fonts: z
      .array(
        z.object({
          family: z
            .string()
            .min(1)
            .max(64)
            .regex(/^[\w -]+$/, 'Letters, digits, spaces, underscores and hyphens only'),
          src: assetPath(FONT_EXTENSIONS),
          /** A weight (400) or a range for variable fonts ("100 900"). */
          weight: z
            .string()
            .regex(/^[1-9]00(?: [1-9]00)?$/, 'A weight such as 400, or a range such as 100 900')
            .optional(),
          style: z.enum(['normal', 'italic']).optional(),
        }),
      )
      .max(8)
      .optional(),
  }),
  radius: z.object({ small: radius, medium: radius, large: radius }),
  spacing: z.object({ unit: z.number().min(2).max(8) }),
  background: z.object({
    /** Opacity of `colors.background` over the system vibrancy (1 hides it). */
    baseOpacity: fraction,
    /** Colors of the flowing gradient shown when there is no artwork. */
    gradient: z.array(hexColor).min(2).max(5),
    gradientOpacity: fraction,
    /** Opacity of the blurred artwork behind everything while a track plays. */
    artworkOpacity: fraction,
    /** Replace the gradient with colors from the playing track's artwork. */
    useArtwork: z.boolean(),
    /** A picture from the theme behind everything, under the artwork while a track plays. */
    image: z
      .object({
        src: assetPath(IMAGE_EXTENSIONS),
        opacity: fraction,
        /** `cover` fills the window, `tile` repeats the picture at its own size. */
        fit: z.enum(['cover', 'contain', 'tile']),
      })
      .optional(),
  }),
  /** Take the accent color from the playing track's artwork. */
  accentFromArtwork: z.boolean(),
  /** How covers look. Left out: square, still, the placeholder in a soft text color. */
  artwork: z
    .object({
      /** Square (the theme's corner radii), well rounded, or round. */
      shape: z.enum(['square', 'rounded', 'circle']),
      /** The playing track's covers are records: round, with grooves, turning while it plays. */
      vinyl: z.boolean(),
      /** The color of the symbol shown where a track has no cover. */
      placeholderColor: hexColor.optional(),
    })
    .optional(),
  motion: z.object({
    /** Base transition duration in ms. */
    duration: z.number().int().min(0).max(2000),
    easing: easingName,
    /** Speed of the background gradient flow; 0 stops it. */
    backgroundSpeed: z.number().min(0).max(4),
  }),
  /** A layout the theme suggests; applying it is the user's choice. */
  layout: layoutSchema.optional(),
});

export type Theme = z.infer<typeof themeSchema>;
export type Appearance = Theme['appearance'];
