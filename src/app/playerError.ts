import type { TFunction } from 'i18next';

import { knownLinkError } from '../features/links/errors.ts';
import type { PlayerError } from '../lib/player.ts';

/** What to tell the user about a player error. A link that failed for a reason they can fix
 * (cookies, an age limit, a sign-in) gets the same text as in the link dialog. */
export function playerErrorText(error: PlayerError, t: TFunction): string {
  switch (error.kind) {
    case 'mpvMissing':
      return t('player.errors.mpvMissing');
    case 'mpvFailed':
      return t('player.errors.mpvFailed', { message: error.message });
    case 'playback':
      return (
        knownLinkError(error.message, t) ?? t('player.errors.playback', { message: error.message })
      );
  }
}
