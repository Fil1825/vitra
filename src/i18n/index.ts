import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';

import type { Language } from '../lib/settings.ts';
import en from './locales/en.json';
import ru from './locales/ru.json';

export const resources = {
  en: { translation: en },
  ru: { translation: ru },
} as const;

export type Locale = keyof typeof resources;

declare module 'i18next' {
  interface CustomTypeOptions {
    // Makes `t('missing.key')` a type error.
    resources: (typeof resources)['en'];
  }
}

/** Picks the UI language: the setting, or the first supported system language. */
export function resolveLocale(setting: Language, systemLanguages: readonly string[]): Locale {
  if (setting !== 'system') {
    return setting;
  }
  const match = systemLanguages
    .map((tag) => tag.toLowerCase().split('-')[0])
    .find((lang): lang is Locale => lang !== undefined && lang in resources);
  return match ?? 'en';
}

export async function initI18n(locale: Locale): Promise<void> {
  await i18n.use(initReactI18next).init({
    resources,
    lng: locale,
    fallbackLng: 'en',
    interpolation: { escapeValue: false }, // React already escapes
  });
}

export { i18n };
