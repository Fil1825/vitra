import { z } from 'zod';

import { themeSchema } from './schema.ts';

/** Where the published schema lives, for editors that validate `theme.json` against it. */
export const THEME_SCHEMA_URL =
  'https://raw.githubusercontent.com/fil1825/vitra/main/docs/theme.schema.json';

/** The JSON Schema of `theme.json`, generated from the zod schema (docs/theme.schema.json). */
export function themeJsonSchema(): Record<string, unknown> {
  return {
    ...z.toJSONSchema(themeSchema, { target: 'draft-2020-12' }),
    $id: THEME_SCHEMA_URL,
    title: 'Vitra theme',
    description:
      'theme.json of a .vitratheme package: colors, glass, fonts, radii, spacing, background, motion and an optional layout. See docs/THEMES.md.',
  };
}
