import { useEffect, useState } from 'react';

import { bus } from '../../lib/bus.ts';
import { importer, type ImportProgress } from '../../lib/import.ts';
import { report } from '../library/actions.ts';

/** How far the search for an imported playlist's tracks got; follows `import:progress`. */
export function useImportProgress(playlistId: number): ImportProgress | null {
  const [progress, setProgress] = useState<{ id: number; value: ImportProgress } | null>(null);

  useEffect(() => {
    let current = true;
    importer
      .progress(playlistId)
      .then(
        (value) => current && setProgress({ id: playlistId, value }),
        report('Reading import progress'),
      );
    const off = bus.on('import:progress', ({ playlistId: id, ...value }) => {
      if (id === playlistId) setProgress({ id, value });
    });
    return () => {
      current = false;
      off();
    };
  }, [playlistId]);

  return progress?.id === playlistId ? progress.value : null;
}
