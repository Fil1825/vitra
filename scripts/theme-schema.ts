// Writes docs/theme.schema.json from the zod schema (src/theme/schema.ts), the single source.
// A test fails while the committed file is stale. Run: npm run theme-schema
import { writeFileSync } from 'node:fs';

import { themeJsonSchema } from '../src/theme/jsonSchema.ts';

const target = new URL('../docs/theme.schema.json', import.meta.url);
writeFileSync(target, `${JSON.stringify(themeJsonSchema(), null, 2)}\n`);
console.log(`wrote ${target.pathname}`);
