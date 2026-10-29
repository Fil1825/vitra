import { create } from 'zustand';

import { wave, type StartOutcome, type WaveMode } from '../../lib/wave.ts';
import { useNavigation } from '../../stores/navigation.ts';

const report = (what: string) => (err: unknown) => console.error(`${what} failed`, err);

interface WaveUi {
  /** The mode being started (the button shows it working: a track may be measured first). */
  starting: WaveMode | null;
  /** Why the last start didn't happen, shown on the wave's page. */
  notice: Exclude<StartOutcome, 'started'> | null;
}

export const useWaveUi = create<WaveUi>(() => ({ starting: null, notice: null }));

/**
 * Starts My Wave in this mood. When nothing could be picked, the wave's page says why (the
 * analysis hasn't measured anything yet, or the track couldn't be measured).
 */
export async function startWave(mode: WaveMode): Promise<void> {
  if (useWaveUi.getState().starting) {
    return;
  }
  useWaveUi.setState({ starting: mode, notice: null });
  try {
    const outcome = await wave.start(mode);
    if (outcome !== 'started') {
      useWaveUi.setState({ notice: outcome });
      useNavigation.getState().go({ view: 'wave' });
    }
  } catch (err) {
    report('Starting My Wave')(err);
  } finally {
    useWaveUi.setState({ starting: null });
  }
}

export function dislikeTrack(trackId: number): void {
  wave.dislike(trackId).catch(report('Disliking'));
}

export function stopWave(): void {
  wave.stop().catch(report('Stopping My Wave'));
}
