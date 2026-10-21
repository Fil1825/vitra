import { useCallback, useEffect, useRef, useState } from 'react';

import { bus } from './bus.ts';
import type { LibraryChange } from './library.ts';

/** Last result per key, so going back to a view shows it at once while it refreshes. */
const cache = new Map<string, unknown>();

/** Bursts of change events (a scan reports every few seconds) cause one refetch. */
const COALESCE_MS = 250;

const TRACKS: readonly LibraryChange[] = ['tracks'];

export interface LibraryData<T> {
  data: T | undefined;
  error: unknown;
  reload(): void;
}

/**
 * Loads library data for `key` and reloads it when the backend reports a matching change.
 * `load` may change every render; only `key` decides when to load.
 */
export function useLibraryData<T>(
  key: string | null,
  load: () => Promise<T>,
  changes: readonly LibraryChange[] = TRACKS,
): LibraryData<T> {
  const [result, setResult] = useState<{ key: string | null; data?: T; error?: unknown }>({
    key: null,
  });
  const [version, setVersion] = useState(0);
  const loadRef = useRef(load);
  useEffect(() => {
    loadRef.current = load;
  });

  useEffect(() => {
    if (key === null) {
      return;
    }
    let cancelled = false;
    loadRef.current().then(
      (data) => {
        if (!cancelled) {
          cache.set(key, data);
          setResult({ key, data });
        }
      },
      (error: unknown) => {
        if (!cancelled) {
          console.error(`Loading ${key} failed`, error);
          setResult({ key, error });
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, [key, version]);

  const changeKey = changes.join(',');
  useEffect(() => {
    let timer: number | undefined;
    const off = bus.on('library:changed', (change) => {
      if (changeKey.split(',').includes(change) && timer === undefined) {
        timer = window.setTimeout(() => {
          timer = undefined;
          setVersion((v) => v + 1);
        }, COALESCE_MS);
      }
    });
    return () => {
      off();
      window.clearTimeout(timer);
    };
  }, [changeKey]);

  const reload = useCallback(() => setVersion((v) => v + 1), []);
  const current = result.key === key;
  return {
    data: current
      ? (result.data ?? (cache.get(key ?? '') as T | undefined))
      : (cache.get(key ?? '') as T | undefined),
    error: current ? result.error : undefined,
    reload,
  };
}
