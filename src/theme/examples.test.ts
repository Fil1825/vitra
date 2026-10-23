import { describe, expect, it } from 'vitest';

import { normalizeLayout } from '../layout/model.ts';
import { sanitizeCss } from './css.ts';
import { themeSchema } from './schema.ts';
import { checkThemeJson, withAssets } from './user.ts';

// The example themes in the repository's `themes/` folder, as files (their packing is
// tested in crates/themes/tests/examples.rs).
const jsons = import.meta.glob<string>('../../themes/*/theme.json', {
  eager: true,
  query: '?raw',
  import: 'default',
});
const styles = import.meta.glob<string>('../../themes/*/custom.css', {
  eager: true,
  query: '?raw',
  import: 'default',
});
const assetFiles = Object.keys(import.meta.glob('../../themes/*/assets/*'));

const folderOf = (path: string) => path.split('/').at(-2) ?? '';
const examples = Object.entries(jsons).map(([path, json]) => {
  const folder = folderOf(path);
  return {
    folder,
    json,
    css: styles[`../../themes/${folder}/custom.css`] ?? null,
    assets: assetFiles
      .filter((file) => file.startsWith(`../../themes/${folder}/assets/`))
      .map((file) => file.split('/').at(-1) ?? ''),
  };
});

describe('example themes', () => {
  it('are there', () => {
    expect(examples.length).toBeGreaterThanOrEqual(2);
  });

  it.each(examples)('$folder follows the schema and uses only its own files', (example) => {
    const checked = checkThemeJson(example.json);
    if (!checked.ok) throw new Error(checked.error);
    const { theme } = checked;
    // Installed under its folder's name, as an import would.
    expect(theme.id).toBe(example.folder);
    // Every font and picture it names is in its assets.
    expect(withAssets(theme, example.assets)).toEqual(theme);
    expect(themeSchema.parse(theme)).toEqual(theme);
    if (theme.layout) {
      expect(normalizeLayout(theme.layout)).toEqual(theme.layout);
    }
  });

  it.each(examples.filter((example) => example.css !== null))(
    '$folder custom.css needs no cleaning',
    (example) => {
      const cleaned = sanitizeCss(example.css ?? '', (path) =>
        example.assets.includes(path.slice('assets/'.length)) ? `theme://${path}` : null,
      );
      expect(cleaned.removed).toEqual([]);
    },
  );
});
