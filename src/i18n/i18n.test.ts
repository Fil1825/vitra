import { describe, expect, it } from 'vitest';
import { createInstance } from 'i18next';

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

function entries(value: unknown, prefix = ''): [string, string][] {
  if (typeof value === 'string') return [[prefix, value]];
  if (typeof value !== 'object' || value === null)
    throw new Error(`Invalid translation: ${prefix}`);
  return Object.entries(value).flatMap(([key, child]) =>
    entries(child, prefix ? `${prefix}.${key}` : key),
  );
}

const plural = /_(zero|one|two|few|many|other)$/;
const placeholders = (text: string) =>
  [...text.matchAll(/{{\s*([^{}]+?)\s*}}/g)].map((match) => match[1]).sort();

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
    const english = new Map(entries(en));
    for (const [key, text] of entries(ru)) {
      const fallback = english.get(key) ?? english.get(key.replace(plural, '_other'));
      expect(fallback, key).toBeDefined();
      expect(placeholders(text), key).toEqual(placeholders(fallback!));
    }
  });

  it('have nonempty values and complete native plural families', () => {
    for (const [locale, resource] of Object.entries({ en, ru })) {
      const all = new Map(entries(resource));
      const families = new Set<string>();
      for (const [key, text] of all) {
        expect(text.trim(), `${locale}:${key}`).not.toBe('');
        if (plural.test(key)) families.add(key.replace(plural, ''));
      }
      const forms = new Intl.PluralRules(locale).resolvedOptions().pluralCategories;
      for (const family of families) {
        for (const form of forms)
          expect(all.has(`${family}_${form}`), `${locale}:${family}_${form}`).toBe(true);
      }
    }
  });

  it('render every plural family without leaking keys or placeholders', async () => {
    for (const [locale, resource] of Object.entries({ en, ru })) {
      const translator = createInstance();
      await translator.init({
        lng: locale,
        resources: { [locale]: { translation: resource } },
        fallbackLng: false,
      });
      const families = new Set(
        entries(resource)
          .filter(([key]) => plural.test(key))
          .map(([key]) => key.replace(plural, '')),
      );
      for (const family of families) {
        const parameters = Object.fromEntries(
          entries(resource)
            .filter(([key]) => key.replace(plural, '') === family)
            .flatMap(([, text]) => placeholders(text).map((name) => [name, 'example'])),
        );
        for (const count of [0, 1, 2, 5, 11, 21, 22, 25, 101]) {
          const text = translator.t(family, { ...parameters, count });
          expect(text, `${locale}:${family}:${count}`).not.toBe(family);
          expect(text, `${locale}:${family}:${count}`).not.toContain('{{');
        }
      }
    }
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
