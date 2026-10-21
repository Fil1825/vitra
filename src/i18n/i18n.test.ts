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
    // Plural forms differ by language (Russian has "few" and "many").
    const base = (all: string[]) =>
      [...new Set(all.map((key) => key.replace(/_(zero|one|two|few|many|other)$/, '')))].sort();
    expect(base(keys(ru))).toEqual(base(keys(en)));
  });

  it('give Russian every plural form it needs', () => {
    expect(ru.library.songCount_few).toContain('{{count}}');
    expect(ru.library.songCount_many).toContain('{{count}}');
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
