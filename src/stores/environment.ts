import { create } from 'zustand';

import { browserProbe, detectRefractionSupport } from '../theme/effects.ts';
import { getWindowEffects, type WindowEffects } from '../lib/window.ts';

interface EnvironmentState extends WindowEffects {
  /** The engine can render SVG filters in `backdrop-filter` (see `detectRefractionSupport`). */
  supportsRefraction: boolean;
}

/** Facts about the platform and engine; set once at startup. */
export const useEnvironment = create<EnvironmentState>(() => ({
  platform: 'web',
  vibrancy: 'none',
  supportsRefraction: false,
}));

export async function initEnvironment(): Promise<void> {
  const effects = await getWindowEffects().catch((err: unknown) => {
    console.error('Reading window effects failed', err);
    return { platform: 'web', vibrancy: 'none' } satisfies WindowEffects;
  });
  useEnvironment.setState({
    ...effects,
    supportsRefraction: detectRefractionSupport(browserProbe()),
  });
}
