import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { system } from '../lib/system.ts';

/**
 * Hands the menu-bar icon's words to Rust, which builds its menu, whenever the language
 * changes. The main window does it: it is always there, even while hidden.
 */
export function useTrayLabels(): void {
  const { t, i18n } = useTranslation();
  const language = i18n.resolvedLanguage;

  useEffect(() => {
    system
      .setTrayLabels({
        notPlaying: t('tray.notPlaying'),
        play: t('tray.play'),
        pause: t('tray.pause'),
        next: t('tray.next'),
        previous: t('tray.previous'),
        show: t('tray.show'),
        miniPlayer: t('tray.miniPlayer'),
        quit: t('tray.quit'),
      })
      .catch((err: unknown) => console.error('Translating the tray menu failed', err));
    // `t` follows the language.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [language]);
}
