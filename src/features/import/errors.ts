import type { ParseKeys, TFunction } from 'i18next';

import { knownLinkError } from '../links/errors.ts';

/** English messages of the import commands (src-tauri/src/import) and their translations. */
const EXACT: Record<string, ParseKeys> = {
  'no tracks in the text': 'import.errors.noTracks',
  'not a link to a playlist': 'import.errors.notList',
  "Vitra can't read lists from this link": 'import.errors.notList',
  'the short link leads nowhere Vitra knows': 'import.errors.shortLink',
  'the site did not answer in time': 'import.errors.timeout',
  'the playlist is no longer in the Music app': 'import.errors.musicGone',
};

const PREFIXES: [string, ParseKeys][] = [
  ['VK shows its playlists only', 'import.errors.vk'],
  ['the service has no such playlist', 'import.errors.notFound'],
  ['the playlist is private', 'import.errors.private'],
  ["this link can't be read right now", 'import.errors.unreadable'],
  ['cannot reach the site', 'import.errors.offline'],
  ['cannot run curl', 'import.errors.offline'],
  ['cannot read the Music library', 'import.errors.musicAccess'],
];

/** A readable message for a failed import (the backend sends English text). */
export function importError(err: unknown, t: TFunction): string {
  const message = err instanceof Error ? err.message : String(err);
  const exact = EXACT[message];
  if (exact !== undefined) {
    return t(exact);
  }
  const prefix = PREFIXES.find(([start]) => message.startsWith(start));
  if (prefix !== undefined) {
    return t(prefix[1]);
  }
  return knownLinkError(message, t) ?? t('import.errors.failed', { message });
}
