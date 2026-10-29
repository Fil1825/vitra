import { useEffect, useState } from 'react';

import { bus } from '../../lib/bus.ts';
import { NO_ANALYSIS, wave, type AnalysisStatus } from '../../lib/wave.ts';
import { report } from '../library/actions.ts';

/** How far My Wave's background analysis has come; follows `wave:progress`. */
export function useWaveAnalysis(): AnalysisStatus {
  const [status, setStatus] = useState<AnalysisStatus>(NO_ANALYSIS);

  useEffect(() => {
    let current = true;
    // An event may come before the answer: it is newer, so the answer doesn't overwrite it.
    let heard = false;
    wave
      .analysis()
      .then((value) => current && !heard && setStatus(value), report('Reading the analysis'));
    const off = bus.on('wave:progress', (value) => {
      heard = true;
      setStatus(value);
    });
    return () => {
      current = false;
      off();
    };
  }, []);

  return status;
}
