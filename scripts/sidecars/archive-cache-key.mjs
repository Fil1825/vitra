import { SIDECARS, planFor } from './plan.mjs';

/** Paths that change pinned digests; keep in sync with GitHub Actions hashFiles(). */
export const SIDECAR_MANIFEST_PATHS = [
  'scripts/sidecars/plan.mjs',
  'scripts/sidecars/ytdlp-release.mjs',
];

/** GitHub Actions cache keys: alphanumeric, `-`, `_` only (no commas). */
export const GITHUB_ACTIONS_CACHE_KEY_PATTERN = /^[a-zA-Z0-9_-]+$/;
export const GITHUB_ACTIONS_CACHE_KEY_MAX = 512;

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
  return selected.join('_');
}

export function assertActionsCacheKey(key) {
  if (key.length > GITHUB_ACTIONS_CACHE_KEY_MAX) {
    throw new Error(`cache key length ${key.length} exceeds ${GITHUB_ACTIONS_CACHE_KEY_MAX}`);
  }
  if (!GITHUB_ACTIONS_CACHE_KEY_PATTERN.test(key)) {
    throw new Error(`cache key fails GitHub Actions validation: ${key}`);
  }
}

/**
 * GitHub Actions cache key for verified raw archives produced by one fetch invocation.
 * @param {string} manifestHash - output of hashFiles(SIDECAR_MANIFEST_PATHS)
 * @param {string} target - Rust triple passed to fetch-sidecars
 * @param {string} scope - from {@link sidecarCacheScope}
 */
export function actionsArchiveCacheKey(manifestHash, target, scope, version = 'v3') {
  const safeScope = scope.replace(/,/g, '_').replace(/[^a-zA-Z0-9_-]+/g, '-');
  const key = `sidecar-archives-${version}-${manifestHash}-${target}-${safeScope}`;
  assertActionsCacheKey(key);
  return key;
}

/** Pinned SHA-256 values for archives that a fetch may download (for tests and diagnostics). */
export function expectedArchiveDigests(target, only = [], skip = []) {
  const selected = only.length > 0 ? only : SIDECARS.filter((name) => !skip.includes(name));
  const plan = planFor(target);
  const digests = [];
  for (const name of selected) {
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
