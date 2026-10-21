import { describe, expect, it } from 'vitest';

import { resolveLocale } from './index.ts';
import en from './locales/en.json';
import ru from './locales/ru.json';

function keys(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) {
    return [prefix];
  }
  return Object.entries(value).flatMap(([key, child]) =>
    keys(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe('locales', () => {
  it('English and Russian have the same keys', () => {
    expect(keys(ru).sort()).toEqual(keys(en).sort());
  });

  it('keep interpolation placeholders in translations', () => {
    expect(ru.settings.glassLayersValue).toContain('{{active}}');
    expect(ru.settings.glassLayersValue).toContain('{{max}}');
  });
});

describe('resolveLocale', () => {
  it('uses an explicit setting', () => {
    expect(resolveLocale('ru', ['en-US'])).toBe('ru');
  });

  it('picks the first supported system language', () => {
    expect(resolveLocale('system', ['de-DE', 'ru-RU', 'en'])).toBe('ru');
    expect(resolveLocale('system', ['en-GB'])).toBe('en');
  });

  it('falls back to English', () => {
    expect(resolveLocale('system', ['ja-JP'])).toBe('en');
    expect(resolveLocale('system', [])).toBe('en');
  });
});
