import { SIDECARS, planFor } from './plan.mjs';

/** Paths that change pinned digests; keep in sync with GitHub Actions hashFiles(). */
export const SIDECAR_MANIFEST_PATHS = [
  'scripts/sidecars/plan.mjs',
  'scripts/sidecars/ytdlp-release.mjs',
];

/**
 * Normalizes the sidecars selected by fetch-sidecars `--only` / `--skip`.
 * Jobs must pass a distinct scope so Actions cache entries are not shared across
 * partial fetches (cache save is immutable per key).
 */
export function sidecarCacheScope(only = [], skip = []) {
  const onlySet = only.length ? new Set(only) : null;
  const skipSet = new Set(skip);
  const selected = SIDECARS.filter(
    (name) => (onlySet ? onlySet.has(name) : true) && !skipSet.has(name),
  );
  return selected.join(',');
}

/**
 * GitHub Actions cache key for verified raw archives produced by one fetch invocation.
 * @param {string} manifestHash - output of hashFiles(SIDECAR_MANIFEST_PATHS)
 * @param {string} target - Rust triple passed to fetch-sidecars
 * @param {string} scope - from {@link sidecarCacheScope}
 */
export function actionsArchiveCacheKey(manifestHash, target, scope) {
  const safeScope = scope.replace(/[^a-z0-9,_-]+/gi, '-');
  return `sidecar-archives-v2-${manifestHash}-${target}-${safeScope}`;
}

/** Pinned SHA-256 values for archives that a fetch may download (for tests and diagnostics). */
export function expectedArchiveDigests(target, only = [], skip = []) {
  const scope = sidecarCacheScope(only, skip);
  const onlySet = scope === '' ? [] : scope.split(',');
  const plan = planFor(target);
  const digests = [];
  for (const name of onlySet.length ? onlySet : SIDECARS) {
    const sidecar = plan[name];
    if (!sidecar || sidecar.type !== 'download') continue;
    for (const download of sidecar.downloads) {
      if (download.checksum?.type === 'pinned') {
        digests.push({ sidecar: name, name: download.name, sha256: download.checksum.sha256 });
      }
    }
  }
  return digests;
}
