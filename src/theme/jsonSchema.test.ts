import { describe, expect, it } from 'vitest';

import committed from '../../docs/theme.schema.json';
import { themeJsonSchema } from './jsonSchema.ts';

describe('docs/theme.schema.json', () => {
  it('matches the zod schema (run `npm run theme-schema` after changing it)', () => {
    expect(committed).toEqual(themeJsonSchema());
  });
});
