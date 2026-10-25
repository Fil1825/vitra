import { useEffect, useState } from 'react';

import { bus } from '../../lib/bus.ts';
import { importer, type MatcherStatus } from '../../lib/import.ts';
import { report } from '../library/actions.ts';

/** What the background search is doing; follows `import:status`. */
export function useMatcherStatus(): MatcherStatus {
  const [status, setStatus] = useState<MatcherStatus>({ state: 'idle' });

  useEffect(() => {
    let current = true;
    // An event may come before the answer: it is newer, so the answer doesn't overwrite it.
    let heard = false;
    importer
      .status()
      .then((value) => current && !heard && setStatus(value), report('Reading the import status'));
    const off = bus.on('import:status', (value) => {
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

/** Whole seconds until `at` (Unix ms), ticking while there are any. */
export function useSecondsUntil(at: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (at === null) {
      return;
    }
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [at]);
  return at === null ? 0 : Math.max(0, Math.ceil((at - now) / 1000));
}
