import * as lucide from 'lucide-react';
import { describe, expect, it } from 'vitest';

// As text (Vitest's `css.include` lets this one through).
const css =
  Object.values(
    import.meta.glob<string>('./icons.css', { eager: true, query: '?inline', import: 'default' }),
  )[0] ?? '';

// Every source file, to find the icons the app shows.
const sources = import.meta.glob<string>(['../**/*.tsx', '../**/*.ts', '!../**/*.test.*'], {
  eager: true,
  query: '?raw',
  import: 'default',
});

/** Lucide's class for an icon component: `Settings2` → `settings-2` (aliases resolved). */
function iconClass(name: string): string {
  const component = (lucide as unknown as Record<string, { displayName?: string } | undefined>)[
    name
  ];
  const pascal = component?.displayName ?? name;
  return pascal
    .split(/(?=[A-Z])|(?<=[a-z])(?=\d)/)
    .join('-')
    .toLowerCase();
}

function usedIcons(): Set<string> {
  const names = new Set<string>();
  for (const source of Object.values(sources)) {
    for (const match of source.matchAll(/import\s*\{([^}]*)\}\s*from\s*'lucide-react'/g)) {
      for (const part of (match[1] ?? '').split(',')) {
        const name = part.trim().replace(/^type\s+/, '');
        if (name && !name.startsWith('Lucide')) names.add(name);
      }
    }
  }
  return names;
}

describe('icons', () => {
  it('each move in their own way under the pointer', () => {
    const icons = usedIcons();
    expect(icons.size).toBeGreaterThan(40);
    const still = [...icons]
      .map(iconClass)
      // The spinner turns all the time already.
      .filter((cls) => cls !== 'loader-circle')
      .filter((cls) => !new RegExp(`\\.lucide-${cls}[\\s,>{]`).test(css));
    expect(still).toEqual([]);
  });

  it('names classes the way Lucide does', () => {
    expect(iconClass('Settings2')).toBe('settings-2');
    expect(iconClass('AlertTriangle')).toBe('triangle-alert');
    expect(iconClass('AArrowDown')).toBe('a-arrow-down');
    expect(iconClass('PictureInPicture2')).toBe('picture-in-picture-2');
  });
});
