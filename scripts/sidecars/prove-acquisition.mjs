#!/usr/bin/env node
/**
 * Network evidence helper (not part of default npm test). Runs fetch-sidecars with an empty
 * content-addressed archive cache and records cold/warm/poisoned behavior.
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import { archiveCachePath, readVerifiedArchive, sha256File } from './archive-cache.mjs';
import { expectedArchiveDigests } from './archive-cache-key.mjs';
import { planFor } from './plan.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const FETCH = path.join(ROOT, 'scripts/fetch-sidecars.mjs');

const { values: args } = parseArgs({
  options: {
    target: { type: 'string', default: 'x86_64-unknown-linux-gnu' },
    only: { type: 'string', default: 'ffmpeg' },
    'require-checksums': { type: 'boolean', default: true },
  },
});

function jsonReport(label, payload) {
  console.log(JSON.stringify({ label, at: new Date().toISOString(), ...payload }, null, 2));
}

function runFetch(target, only, cacheRoot, outDir) {
  const argv = [FETCH, '--target', target, '--only', only, '--force', '--out', outDir];
  if (args['require-checksums']) {
    argv.push('--require-checksums');
  }
  const result = spawnSync(process.execPath, argv, {
    cwd: ROOT,
    env: { ...process.env, VITRA_SIDECAR_ARCHIVE_CACHE: cacheRoot },
    encoding: 'utf8',
    timeout: 600_000,
  });
  return {
    exit: result.status,
    signal: result.signal,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
  };
}

function pinnedDigests(target, onlyList) {
  const only = onlyList
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  const digests = expectedArchiveDigests(target, only);
  const plan = planFor(target);
  const sources = [];
  for (const name of only) {
    const sidecar = plan[name];
    if (sidecar?.type !== 'download') continue;
    for (const d of sidecar.downloads) {
      sources.push({
        sidecar: name,
        name: d.name,
        pinnedSha256: d.checksum?.sha256 ?? null,
        githubRelease: d.githubRelease ?? null,
        browserUrls: d.urls,
        checksumType: d.checksum?.type,
      });
    }
  }
  return { digests, sources };
}

function main() {
  const cacheRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-prove-cache-'));
  const outDir = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-prove-out-'));
  const only = args.only;
  const { digests, sources } = pinnedDigests(args.target, only);

  jsonReport('setup', {
    target: args.target,
    only,
    emptyCacheRoot: cacheRoot,
    outDir,
    expectedPinnedArchives: digests,
    plannedSources: sources,
  });

  const cold = runFetch(args.target, only, cacheRoot, outDir);
  const coldFailed = cold.exit !== 0;
  const httpHints = [...cold.stderr.matchAll(/HTTP (\d{3})/g)].map((m) => m[1]);
  jsonReport('cold-fetch-sidecars', {
    exit: cold.exit,
    downloaded: !coldFailed,
    httpStatusesSeen: [...new Set(httpHints)],
    summaryLine: cold.stderr.split('\n').find((line) => line.includes('Summary:')) ?? null,
    tail: cold.stderr.trim().split('\n').slice(-8),
  });

  if (coldFailed || !digests[0]) {
    jsonReport('warm-fetch', {
      skipped: true,
      reason: coldFailed ? 'cold fetch failed' : 'no pinned digest',
    });
    jsonReport('poisoned-cache', { skipped: true, reason: 'no verified archive to poison' });
    process.exitCode = coldFailed ? 1 : 0;
    return;
  }

  const { sha256, name } = digests[0];
  const warmFile = path.join(cacheRoot, `warm-${name}`);
  const warmHit = readVerifiedArchive(cacheRoot, sha256, warmFile);
  jsonReport('warm-archive-cache', {
    cacheHit: warmHit,
    sha256: warmHit ? sha256File(warmFile) : null,
    matchesPinned: warmHit ? sha256File(warmFile) === sha256 : false,
  });

  const secondOut = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-prove-out2-'));
  const warmRun = runFetch(args.target, only, cacheRoot, secondOut);
  const warmLog = `${warmRun.stdout}\n${warmRun.stderr}`;
  jsonReport('warm-fetch-sidecars', {
    exit: warmRun.exit,
    sawCacheLine: /verified archive cache/.test(warmLog),
    tail: warmLog.trim().split('\n').slice(-6),
  });

  const poisonRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-prove-poison-'));
  const cached = archiveCachePath(cacheRoot, sha256);
  if (fs.existsSync(cached)) {
    const poisoned = archiveCachePath(poisonRoot, sha256);
    fs.mkdirSync(path.dirname(poisoned), { recursive: true });
    fs.writeFileSync(poisoned, 'tampered-bytes');
    const poisonDest = path.join(poisonRoot, 'out.bin');
    try {
      readVerifiedArchive(poisonRoot, sha256, poisonDest);
      jsonReport('poisoned-cache', { rejected: false });
    } catch (err) {
      jsonReport('poisoned-cache', { rejected: true, error: err.message });
    }
  }
}

main();
