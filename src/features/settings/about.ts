/**
 * What the About page says in the author's own words. Fill these in:
 *
 * - `AUTHOR_NOTE`: a few words from the author, per interface language. When one language is
 *   left empty the other's text shows; when both are, the note isn't shown.
 * - `DONATE_URL`: where the "Support Vitra" button leads (an `https://` page). The button
 *   is hidden while it is empty.
 */
export const AUTHOR_NOTE: Record<'en' | 'ru', string> = {
  en: '',
  ru: '',
};

export const DONATE_URL = '';

export const REPOSITORY_URL = 'https://github.com/Fil1825/vitra';

/** The note in `language`, else in the other language, else nothing. */
export function authorNote(language: string): string {
  const own = language.startsWith('ru') ? AUTHOR_NOTE.ru : AUTHOR_NOTE.en;
  return (own || AUTHOR_NOTE.ru || AUTHOR_NOTE.en).trim();
}
