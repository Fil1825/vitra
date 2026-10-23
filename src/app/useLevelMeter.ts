import { useEffect, useState } from 'react';

import { isShown } from '../layout/model.ts';
import { useLayout } from '../layout/state.ts';
import { player } from '../lib/player.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useSettings } from '../stores/settings.ts';

/**
 * Asks the backend for the music's level while the window is visible and something shows
 * it: the backdrop's pulse (the setting is on and motion is allowed: not Reduce Motion, not
 * power saving) or the level meter module. Reading the level costs a little, so it stops
 * otherwise.
 */
export function useLevelMeter(): void {
  const enabled = useSettings((s) => s.settings.appearance.backdropPulse);
  const moving = useAppearance((s) => s.effects.animatedBackdrop);
  const meter = isShown(useLayout(), 'level');
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');

  useEffect(() => {
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  const wanted = visible && ((enabled && moving) || meter);
  useEffect(() => {
    player
      .levelMeter(wanted)
      .catch((err: unknown) => console.error('Switching the level meter failed', err));
  }, [wanted]);
}
