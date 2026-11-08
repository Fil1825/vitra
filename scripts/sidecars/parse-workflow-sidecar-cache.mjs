import {
  assertActionsCacheKey,
  actionsArchiveCacheKey,
  sidecarCacheScope,
} from './archive-cache-key.mjs';

/** Must match `hashFiles(...)` used in restore/save steps exactly. */
export const WORKFLOW_HASHFILES =
  "${{ hashFiles('scripts/sidecars/plan.mjs', 'scripts/sidecars/ytdlp-release.mjs') }}";

const RESTORE_KEY_LINE = new RegExp(
  String.raw`^\s*key:\s*sidecar-archives-v3-\$\{\{\s*hashFiles\('scripts/sidecars/plan\.mjs',\s*'scripts/sidecars/ytdlp-release\.mjs'\)\s*\}\}-(.+)\s*$`,
);

const RUN_TRIPLE_PREFIX =
  /^(x86_64-unknown-linux-gnu|aarch64-apple-darwin|x86_64-pc-windows-msvc|\$\{\{\s*matrix\.target\s*\}\})-/;

/** @returns {{ line: number, keySuffix: string, fetchRun: string }[]} */
export function extractSidecarCachePairs(workflowText) {
  const lines = workflowText.split('\n');
  const pairs = [];
  for (let i = 0; i < lines.length; i++) {
    const match = RESTORE_KEY_LINE.exec(lines[i]);
    if (!match) continue;
    let fetchRun = '';
    for (let j = i + 1; j < Math.min(i + 30, lines.length); j++) {
      const runMatch = lines[j].match(/^\s*run:\s*(.+)\s*$/);
      if (runMatch && /fetch-sidecars/.test(runMatch[1])) {
        fetchRun = runMatch[1].trim();
        break;
      }
    }
    pairs.push({ line: i + 1, keySuffix: match[1].trim(), fetchRun });
  }
  return pairs;
}

export function parseFetchSidecarsRun(run) {
  if (!run) return { target: null, only: null, full: true };
  const target = /--target\s+(\S+)/.exec(run)?.[1] ?? null;
  const onlyMatch = /--only\s+([^\s]+)/.exec(run);
  const only = onlyMatch ? onlyMatch[1].split(',').map((s) => s.trim()) : null;
  return { target, only, full: !onlyMatch };
}

export function expectedCacheKeySuffix(run, defaultTarget) {
  if (run.includes('${{ matrix.target }}')) {
    return `\${{ matrix.target }}-${sidecarCacheScope([])}`;
  }
  const { target, only, full } = parseFetchSidecarsRun(run);
  const resolvedTarget = target ?? defaultTarget;
  if (!resolvedTarget) {
    throw new Error('cannot derive cache key suffix without --target or defaultTarget');
  }
  const scope = full ? sidecarCacheScope([]) : sidecarCacheScope(only);
  return `${resolvedTarget}-${scope}`;
}

function inferDefaultTarget(keySuffix) {
  return RUN_TRIPLE_PREFIX.exec(keySuffix)?.[1] ?? null;
}

export function materializeActionsKey(keySuffix) {
  const concrete = keySuffix.replace(/\$\{\{\s*matrix\.target\s*\}\}/g, 'aarch64-apple-darwin');
  return `sidecar-archives-v3-manifestsha-${concrete}`;
}

/**
 * Validates every sidecar cache/restore pair in a workflow file.
 * @param {string} workflowText - contents of ci.yml or release.yml
 * @param {string} label - path for error messages
 */
export function validateWorkflowSidecarCaches(workflowText, label) {
  const pairs = extractSidecarCachePairs(workflowText);
  if (pairs.length === 0) {
    throw new Error(`${label}: no sidecar-archives-v3 cache restore keys found`);
  }
  const errors = [];
  for (const pair of pairs) {
    const { keySuffix, fetchRun, line } = pair;
    if (keySuffix.includes(',')) {
      errors.push(`${label}:${line} key suffix contains comma: ${keySuffix}`);
      continue;
    }
    try {
      assertActionsCacheKey(materializeActionsKey(keySuffix));
    } catch (err) {
      errors.push(`${label}:${line} ${err.message}`);
    }
    if (!fetchRun) {
      errors.push(`${label}:${line} missing fetch-sidecars run after cache restore key`);
      continue;
    }
    const defaultTarget = inferDefaultTarget(keySuffix);
    try {
      const expected = expectedCacheKeySuffix(
        fetchRun,
        defaultTarget && !defaultTarget.includes('matrix') ? defaultTarget : undefined,
      );
      if (keySuffix !== expected) {
        errors.push(
          `${label}:${line} key suffix "${keySuffix}" does not match fetch scope (expected "${expected}")`,
        );
      }
    } catch (err) {
      errors.push(`${label}:${line} ${err.message}`);
    }
  }
  if (errors.length) {
    throw new Error(errors.join('\n'));
  }
  return pairs;
}
