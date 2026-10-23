import { useEffect, useState } from 'react';

import { player } from '../lib/player.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useSettings } from '../stores/settings.ts';

/**
 * Asks the backend for the music's pulse while the backdrop can show it: the setting is
 * on, motion is allowed (not Reduce Motion, not power saving) and the window is visible.
 * Reading the level costs a little, so it stops otherwise.
 */
export function useLevelMeter(): void {
  const enabled = useSettings((s) => s.settings.appearance.backdropPulse);
  const moving = useAppearance((s) => s.effects.animatedBackdrop);
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');

  useEffect(() => {
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  const wanted = enabled && moving && visible;
  useEffect(() => {
    player
      .levelMeter(wanted)
      .catch((err: unknown) => console.error('Switching the level meter failed', err));
  }, [wanted]);
}
