#!/usr/bin/env node
// Downloads the helper binaries Vitra runs (mpv, yt-dlp, FFmpeg, Deno) into
// src-tauri/sidecars/<target triple>/. They are never committed to the repository.
//
// Usage: npm run fetch-sidecars -- [options]   (run with --help for the list)

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

import {
  SIDECARS,
  SUPPORTED_TARGETS,
  exeName,
  hostTarget,
  parseSha256,
  pickBtbnAsset,
  planFor,
} from './sidecars/plan.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

const HELP = `Fetch Vitra's sidecar binaries.

Usage: npm run fetch-sidecars -- [options]

Options:
  --target <triple>      Target to fetch for (default: this machine).
                         Supported: ${SUPPORTED_TARGETS.join(', ')}
  --only <list>          Comma-separated subset of: ${SIDECARS.join(', ')}
  --skip <list>          Comma-separated sidecars to skip
  --out <dir>            Output directory (default: src-tauri/sidecars/<target>)
  --force                Re-download sidecars that are already present
  --require-checksums    Fail instead of warning when a checksum is unavailable
  -h, --help             Show this help
`;

// Node's fetch ignores HTTPS_PROXY unless NODE_USE_ENV_PROXY is set at startup (Node >= 22.21).
if ((process.env.HTTPS_PROXY || process.env.https_proxy) && !process.env.NODE_USE_ENV_PROXY) {
  const child = spawnSync(
    process.execPath,
    [...process.execArgv, '--disable-warning=UNDICI-EHPA', ...process.argv.slice(1)],
    {
      stdio: 'inherit',
      env: { ...process.env, NODE_USE_ENV_PROXY: '1' },
    },
  );
  process.exit(child.status ?? 1);
}

const { values: args } = parseArgs({
  options: {
    target: { type: 'string' },
    only: { type: 'string' },
    skip: { type: 'string' },
    out: { type: 'string' },
    force: { type: 'boolean', default: false },
    'require-checksums': { type: 'boolean', default: false },
    help: { type: 'boolean', short: 'h', default: false },
  },
});

if (args.help) {
  console.log(HELP);
  process.exit(0);
}

const host = hostTarget();
const target = args.target ?? host;
const isHost = target === host;
const outDir = path.resolve(args.out ?? path.join(ROOT, 'src-tauri', 'sidecars', target));

const list = (value) =>
  value
    ? value
        .split(',')
        .map((s) => s.trim())
        .filter(Boolean)
    : [];
const only = list(args.only);
const skip = list(args.skip);
for (const name of [...only, ...skip]) {
  if (!SIDECARS.includes(name)) {
    fail(`Unknown sidecar "${name}". Known: ${SIDECARS.join(', ')}`);
  }
}
const selected = SIDECARS.filter(
  (s) => (only.length ? only.includes(s) : true) && !skip.includes(s),
);

main().catch((err) => fail(err.message));

async function main() {
  const plan = planFor(target);
  fs.mkdirSync(outDir, { recursive: true });
  log(
    `Target: ${target}${isHost ? '' : ' (cross)'}\nOutput: ${path.relative(ROOT, outDir) || '.'}\n`,
  );

  const manifestPath = path.join(outDir, 'manifest.json');
  const manifest = readJson(manifestPath) ?? { target, sidecars: {} };
  const results = [];

  for (const name of selected) {
    const sidecar = plan[name];
    try {
      if (sidecar.type === 'system') {
        results.push({ name, ...checkSystem(sidecar) });
        continue;
      }
      const present = sidecar.executables.every((e) => fs.existsSync(path.join(outDir, e)));
      if (present && !args.force) {
        results.push({ name, status: 'already present (use --force to update)' });
        continue;
      }
      log(`▸ ${name}`);
      const fetched = await install(sidecar);
      const version = isHost ? probeVersion(sidecar) : null;
      manifest.sidecars[name] = {
        version,
        fetchedAt: new Date().toISOString(),
        downloads: fetched,
      };
      results.push({ name, status: version ? `ok — ${version}` : 'ok' });
    } catch (err) {
      const hint = sidecar.hint ? `\n    ${sidecar.hint}` : '';
      results.push({ name, status: `FAILED: ${err.message}${hint}`, failed: true });
    }
  }

  fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);

  log('\nSummary:');
  for (const r of results) {
    const mark = r.failed ? '✗' : r.warning ? '!' : '✓';
    log(`  ${mark} ${r.name.padEnd(7)} ${r.status}`);
  }
  if (results.some((r) => r.failed)) {
    process.exitCode = 1;
  }
}

/** Downloads, verifies and unpacks every file of one sidecar. */
async function install(sidecar) {
  const workDir = fs.mkdtempSync(path.join(outDir, '.work-'));
  const fetched = [];
  try {
    for (const [i, entry] of sidecar.downloads.entries()) {
      const download = entry.type === 'btbn' ? await resolveBtbn(entry) : entry;
      const file = path.join(workDir, `download-${i}`);
      const { url, sha256 } = await downloadVerified(download, file);
      fetched.push({ name: download.name, url, sha256 });

      let source = file;
      if (download.archive) {
        source = path.join(workDir, `extract-${i}`);
        extract(file, download.archive, source);
        if (download.nested) {
          const nested = findEntry(source, download.nested);
          extract(nested, 'tar', source);
        }
      }
      for (const f of download.files) {
        const from = download.archive && !f.root ? findEntry(source, f.find) : source;
        const to = path.join(outDir, f.to);
        fs.rmSync(to, { recursive: true, force: true });
        fs.mkdirSync(path.dirname(to), { recursive: true });
        // Same filesystem, so a rename keeps symlinks and extended attributes intact.
        fs.renameSync(from, to);
      }
    }
    for (const e of sidecar.executables) {
      const p = path.join(outDir, e);
      if (!fs.existsSync(p)) {
        throw new Error(`expected ${e} after unpacking`);
      }
      if (!target.includes('windows')) {
        fs.chmodSync(p, 0o755);
      }
    }
    return fetched;
  } finally {
    fs.rmSync(workDir, { recursive: true, force: true });
  }
}

async function resolveBtbn(entry) {
  const text = await fetchText(entry.checksumsUrl);
  const picked = pickBtbnAsset(text, entry.platform, entry.ext);
  if (!picked) {
    throw new Error(`no FFmpeg build for ${entry.platform} in ${entry.checksumsUrl}`);
  }
  return {
    ...entry,
    name: picked.asset,
    urls: [`${entry.releaseUrl}/${picked.asset}`],
    checksum: { type: 'pinned', sha256: picked.sha256 },
  };
}

/** Tries each candidate URL until one downloads and passes verification. */
async function downloadVerified(download, file) {
  const errors = [];
  for (const url of download.urls) {
    try {
      const { finalUrl, sha256 } = await downloadFile(url, file, download.name);
      if (download.archive === 'zip' && !hasZipMagic(file)) {
        throw new Error('response is not a zip archive');
      }
      const expected = await expectedSha256(download, finalUrl);
      if (expected && expected !== sha256) {
        throw new Error(`checksum mismatch: expected ${expected}, got ${sha256}`);
      }
      if (!expected) {
        const message = `no checksum published for ${download.name}; sha256 is ${sha256}`;
        if (args['require-checksums']) {
          throw new Error(message);
        }
        warn(message);
      }
      // Record the requested URL: redirect targets are signed and expire.
      return { url, sha256 };
    } catch (err) {
      // fetch() hides the network error (DNS, TLS, ...) in `cause`.
      const cause = err.cause?.message ? ` (${err.cause.message})` : '';
      errors.push(`${url}: ${err.message}${cause}`);
    }
  }
  throw new Error(`could not download ${download.name}\n    ${errors.join('\n    ')}`);
}

async function expectedSha256(download, finalUrl) {
  const { checksum } = download;
  switch (checksum.type) {
    case 'pinned':
      return checksum.sha256;
    case 'sums-file':
      return requireHash(parseSha256(await fetchText(checksum.url), checksum.entry), checksum.url);
    case 'sidecar-file':
      return requireHash(parseSha256(await fetchText(checksum.url)), checksum.url);
    case 'sibling': {
      // Published next to the resolved (timestamped) file, not next to the redirect URL.
      const text = await fetchText(`${finalUrl}${checksum.suffix}`).catch(() => null);
      return text ? parseSha256(text) : null;
    }
    default:
      throw new Error(`unknown checksum type ${checksum.type}`);
  }
}

function requireHash(hash, url) {
  if (!hash) {
    throw new Error(`no checksum found in ${url}`);
  }
  return hash;
}

async function downloadFile(url, file, label) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok || !res.body) {
    throw new Error(`HTTP ${res.status}`);
  }
  const total = Number(res.headers.get('content-length')) || 0;
  const hash = createHash('sha256');
  let received = 0;
  let lastPercent = -1;

  const body = Readable.fromWeb(res.body);
  body.on('data', (chunk) => {
    hash.update(chunk);
    received += chunk.length;
    if (total && process.stdout.isTTY) {
      const percent = Math.floor((received / total) * 100);
      if (percent !== lastPercent) {
        lastPercent = percent;
        process.stdout.write(`\r  ${label} ${percent}% of ${(total / 1e6).toFixed(1)} MB`);
      }
    }
  });
  await pipeline(body, fs.createWriteStream(file));
  if (total && process.stdout.isTTY) {
    process.stdout.write('\n');
  } else {
    log(`  ${label} ${(received / 1e6).toFixed(1)} MB`);
  }
  return { finalUrl: res.url, sha256: hash.digest('hex') };
}

async function fetchText(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} for ${url}`);
  }
  return res.text();
}

function hasZipMagic(file) {
  const fd = fs.openSync(file, 'r');
  try {
    const buf = Buffer.alloc(4);
    fs.readSync(fd, buf, 0, 4, 0);
    return buf.equals(Buffer.from([0x50, 0x4b, 0x03, 0x04]));
  } finally {
    fs.closeSync(fd);
  }
}

function extract(file, format, dest) {
  fs.mkdirSync(dest, { recursive: true });
  if (format === 'zip' && process.platform === 'linux') {
    run('unzip', ['-q', '-o', file, '-d', dest], 'install unzip, e.g. `sudo apt install unzip`');
  } else {
    // bsdtar on macOS and Windows reads zip too; GNU tar on Linux detects xz/gzip itself.
    run(tarCommand(), ['-xf', file, '-C', dest]);
  }
}

function tarCommand() {
  if (process.platform === 'win32') {
    // Prefer Windows' bsdtar over a GNU tar from Git Bash, which cannot read zip files.
    const systemTar = path.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'tar.exe');
    if (fs.existsSync(systemTar)) {
      return systemTar;
    }
  }
  return 'tar';
}

/** Finds a file or directory by name anywhere under `dir` (shallowest match wins). */
function findEntry(dir, name) {
  let level = [dir];
  while (level.length) {
    const next = [];
    for (const d of level) {
      for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
        const p = path.join(d, entry.name);
        if (entry.name === name) {
          return p;
        }
        if (entry.isDirectory() && !entry.name.endsWith('.app')) {
          next.push(p);
        }
      }
    }
    level = next;
  }
  throw new Error(`${name} not found in the downloaded archive`);
}

function checkSystem(sidecar) {
  if (!isHost) {
    return { status: `skipped (comes from the system on ${target})` };
  }
  const found = which(exeName(target, sidecar.binary));
  if (found) {
    return { status: `using system ${sidecar.binary}: ${found}` };
  }
  return { status: `not installed. ${sidecar.hint}`, warning: true };
}

function which(file) {
  for (const dir of (process.env.PATH ?? '').split(path.delimiter)) {
    const p = path.join(dir, file);
    if (dir && fs.existsSync(p)) {
      return p;
    }
  }
  return null;
}

function probeVersion(sidecar) {
  const bin = path.join(outDir, sidecar.executables[0]);
  const res = spawnSync(bin, sidecar.versionArgs, { encoding: 'utf8', timeout: 120_000 });
  if (res.status !== 0) {
    warn(`${bin} ${sidecar.versionArgs.join(' ')} failed: ${res.error?.message ?? res.stderr}`);
    return null;
  }
  return res.stdout.split(/\r?\n/)[0].trim();
}

function run(cmd, cmdArgs, hint) {
  const res = spawnSync(cmd, cmdArgs, { stdio: ['ignore', 'ignore', 'pipe'], encoding: 'utf8' });
  if (res.error) {
    throw new Error(`${cmd} failed: ${res.error.message}${hint ? ` (${hint})` : ''}`);
  }
  if (res.status !== 0) {
    throw new Error(`${cmd} ${cmdArgs.join(' ')} exited with ${res.status}: ${res.stderr.trim()}`);
  }
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch {
    return null;
  }
}

function log(message) {
  console.log(message);
}

function warn(message) {
  console.warn(`  ! ${message}`);
}

function fail(message) {
  console.error(`fetch-sidecars: ${message}`);
  process.exit(1);
}
