import { createInstance } from 'i18next';

import { resolveLocale, resources } from './index.ts';

/** A boot failure may happen before the shared translator has initialized. */
export async function startupFailure(
  error: unknown,
  languages: readonly string[],
): Promise<string> {
  const translator = createInstance();
  await translator.init({
    resources,
    lng: resolveLocale('system', languages),
    fallbackLng: 'en',
    interpolation: { escapeValue: false },
  });
  return translator.t('app.startupFailed', { message: String(error) });
}
