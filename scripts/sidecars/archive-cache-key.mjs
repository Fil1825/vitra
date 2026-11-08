import { SIDECARS, SUPPORTED_TARGETS, planFor } from './plan.mjs';

/** Paths that change pinned digests; keep in sync with GitHub Actions hashFiles(). */
export const SIDECAR_MANIFEST_PATHS = [
  'scripts/sidecars/plan.mjs',
  'scripts/sidecars/ytdlp-release.mjs',
];

/** GitHub Actions cache keys: alphanumeric, `-`, `_` only (no commas). */
export const GITHUB_ACTIONS_CACHE_KEY_PATTERN = /^[a-zA-Z0-9_-]+$/;
export const GITHUB_ACTIONS_CACHE_KEY_MAX = 512;

/** Must match `.github/workflows/ci.yml` and `release.yml` restore/save steps. */
export const WORKFLOW_ARCHIVE_CACHE_SCOPES = [
  { workflow: 'ci.yml', job: 'rust', target: 'x86_64-unknown-linux-gnu', only: ['ffmpeg'] },
  { workflow: 'ci.yml', job: 'ytdlp', target: 'x86_64-unknown-linux-gnu', only: ['yt-dlp'] },
  {
    workflow: 'ci.yml',
    job: 'macos',
    target: 'aarch64-apple-darwin',
    only: ['yt-dlp', 'ffmpeg'],
  },
  {
    workflow: 'ci.yml',
    job: 'windows',
    target: 'x86_64-pc-windows-msvc',
    only: ['mpv', 'yt-dlp', 'ffmpeg', 'deno'],
  },
  ...SUPPORTED_TARGETS.map((target) => ({
    workflow: 'release.yml',
    job: 'build',
    target,
    only: [...SIDECARS],
  })),
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

/** Builds the exact key shape each workflow job uses (manifest hash substituted in CI). */
export function workflowArchiveCacheKey({ target, only }, manifestHash = 'manifestsha') {
  return actionsArchiveCacheKey(manifestHash, target, sidecarCacheScope(only));
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
