import { create } from 'zustand';

import { browserProbe, detectRefractionSupport } from '../theme/effects.ts';
import { bus } from '../lib/bus.ts';
import { NO_WINDOW_EFFECTS, getWindowEffects, type WindowEffects } from '../lib/window.ts';

interface EnvironmentState extends WindowEffects {
  /** The engine can render SVG filters in `backdrop-filter` (see `detectRefractionSupport`). */
  supportsRefraction: boolean;
}

/** Facts about the platform, engine and native window backdrop. */
export const useEnvironment = create<EnvironmentState>(() => ({
  ...NO_WINDOW_EFFECTS,
  supportsRefraction: false,
}));

export async function initEnvironment(): Promise<void> {
  const effects = await getWindowEffects().catch((err: unknown) => {
    console.error('Reading window effects failed', err);
    return NO_WINDOW_EFFECTS;
  });
  useEnvironment.setState({
    ...effects,
    supportsRefraction: detectRefractionSupport(browserProbe()),
  });
  bus.on('window:effects', (effects) => useEnvironment.setState(effects));
}
