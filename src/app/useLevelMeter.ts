import { useEffect, useState } from 'react';

import { isShown } from '../layout/model.ts';
import { useLayout } from '../layout/state.ts';
import { player } from '../lib/player.ts';
import { useAppearance } from '../stores/appearance.ts';
import { useSettings } from '../stores/settings.ts';

/**
 * Asks the backend for what this window shows of the music while it is visible: the pulse
 * for the backdrop (the setting is on and motion is allowed: not Reduce Motion, not power
 * saving) and the spectrum for the level meter module. Reading costs a little, so it stops
 * when neither is on screen.
 */
export function useLevelMeter(): void {
  const pulseSetting = useSettings((s) => s.settings.appearance.backdropPulse);
  const moving = useAppearance((s) => s.effects.animatedBackdrop);
  const meter = isShown(useLayout(), 'level');
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible');

  useEffect(() => {
    const onChange = () => setVisible(document.visibilityState === 'visible');
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  }, []);

  const pulse = visible && pulseSetting && moving;
  const spectrum = visible && meter;
  useEffect(() => {
    player
      .meters(pulse, spectrum)
      .catch((err: unknown) => console.error('Switching the level meters failed', err));
  }, [pulse, spectrum]);
}
