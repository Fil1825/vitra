// Download pinned corresponding-source archives, reject a hash or BtbN pin
// mismatch, and record whether publication is still blocked.
// `prepare` keeps an incomplete kit for diagnostics. `publish` refuses it.
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

import { planFor } from '../sidecars/plan.mjs';
import { publicationBlockers, RELEASE_TARGETS, sourceClosureFor } from './source-kit-gate.mjs';
import sourceOffers from './upstream-source-offers.json' with { type: 'json' };

const SCRIPT_COMMIT = /^SCRIPT_COMMIT="([^"]+)"/m;
const SCRIPT_REV = /^SCRIPT_REV="([^"]+)"/m;
const SCRIPT_REPO = /^SCRIPT_REPO="([^"]+)"/m;

export function sha256(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

export function assertSha256(bytes, expected, label) {
  const got = sha256(bytes);
  if (got !== expected) {
    throw new Error(`checksum mismatch for ${label}: expected ${expected}, got ${got}`);
  }
  return got;
}

/** Split `tar -t` stdout into archive member names.
 *
 * Windows bsdtar list mode does not set stdout to binary (libarchive tar/read.c
 * calls `_setmode(_O_BINARY)` only for extract-to-stdout) and then `fprintf`s a
 * newline. The Universal CRT text mode turns that into CRLF. The disk walker
 * stores `/` (archive_read_disk_windows.c), and listing prints that pathname, so
 * the trailing CR is what makes `endsWith('.sh')` reject every member.
 */
export function tarListLines(stdout) {
  return stdout
    .split('\n')
    .map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line))
    .filter(Boolean);
}

export function listTarEntries(archive) {
  const result = spawnSync('tar', ['-tzf', archive], {
    encoding: 'utf8',
    maxBuffer: 8 * 1024 * 1024,
  });
  if (result.status !== 0) {
    throw new Error(result.stderr || `cannot list ${archive}`);
  }
  return tarListLines(result.stdout);
}

function tarHasEntry(archive, entry) {
  const result = spawnSync('tar', ['-tzf', archive, entry], {
    encoding: 'utf8',
    maxBuffer: 1024 * 1024,
  });
  return result.status === 0 && tarListLines(result.stdout).includes(entry);
}

function tarText(archive, entry) {
  const result = spawnSync('tar', ['-xOzf', archive, entry], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`cannot read ${entry} from ${archive}`);
  }
  return result.stdout;
}

export function scriptEntriesFromTarList(stdout) {
  return tarListLines(stdout).filter(
    (entry) => entry.includes('/scripts.d/') && entry.endsWith('.sh'),
  );
}

/** Dependency pins declared by BtbN script members, given a way to read each one. */
export function pinsFromScriptEntries(entries, readText) {
  const pins = [];
  for (const entry of entries) {
    if (!entry.includes('/scripts.d/') || !entry.endsWith('.sh')) continue;
    const text = readText(entry);
    if (text.includes('SCRIPT_SKIP="1"')) continue;
    const commit = SCRIPT_COMMIT.exec(text);
    const rev = SCRIPT_REV.exec(text);
    const repo = SCRIPT_REPO.exec(text);
    if (!commit && !rev) {
      throw new Error(`BtbN script ${entry} has no SCRIPT_COMMIT or SCRIPT_REV`);
    }
    const pin = { script: entry.split('/scripts.d/')[1], repo: repo?.[1] };
    if (commit) pin.commit = commit[1];
    if (rev) pin.svn = rev[1];
    pins.push(pin);
  }
  pins.sort((a, b) => a.script.localeCompare(b.script));
  return pins;
}

/** Dependency pins declared by a BtbN FFmpeg-Builds tag archive. */
export function pinsFromBtbnArchive(archive) {
  return pinsFromScriptEntries(listTarEntries(archive), (entry) => tarText(archive, entry));
}

function pinKey(pin) {
  return `${pin.script}\0${pin.repo ?? ''}\0${pin.commit ?? ''}\0${pin.svn ?? ''}`;
}

export function assertPinSet(actual, expected) {
  const actualKeys = new Set(actual.map(pinKey));
  const expectedKeys = new Set(expected.map(pinKey));
  const missing = [...expectedKeys].filter((key) => !actualKeys.has(key));
  const extra = [...actualKeys].filter((key) => !expectedKeys.has(key));
  if (missing.length || extra.length) {
    throw new Error(`BtbN pin mismatch: ${missing.length} missing, ${extra.length} unexpected`);
  }
}

/** Throws when the archive's pins differ from the offer manifest. */
export function assertBtbnPins(archive, expected, { branchFile, branch }) {
  assertPinSet(pinsFromBtbnArchive(archive), expected);
  const branchEntry = listTarEntries(archive).find((entry) => entry.endsWith(`/${branchFile}`));
  if (!branchEntry) throw new Error(`BtbN archive has no ${branchFile}`);
  const text = tarText(archive, branchEntry);
  if (!text.includes(`GIT_BRANCH="${branch}"`)) {
    throw new Error(`${branchFile} does not set GIT_BRANCH=${branch}`);
  }
}

export function assertFfmpegCommitArchive(archive, commit) {
  const entries = listTarEntries(archive);
  if (!entries.some((entry) => entry.includes(commit))) {
    throw new Error(`FFmpeg archive does not contain commit ${commit}`);
  }
}

function pinnedSources(offers) {
  const sources = [];
  for (const offer of offers) {
    for (const archive of offer.archives ?? []) {
      if (archive.url && archive.sha256 && archive.name) sources.push(archive);
    }
    for (const dep of offer.dependencies ?? []) {
      if (dep.source?.url && dep.source?.sha256 && dep.source?.name) sources.push(dep.source);
    }
    for (const bundled of offer.bundledSources ?? []) {
      if (bundled.source?.url && bundled.source?.sha256 && bundled.source?.name) {
        sources.push(bundled.source);
      }
    }
  }
  return sources;
}

function offerText(offers) {
  const lines = [
    'Corresponding source collected for this Vitra build.',
    'Publication is refused while SOURCE-KIT-STATUS.json lists blockers.',
    'Hashes are SHA-256 of the archives in this directory.',
    '',
  ];
  for (const offer of offers) {
    const closed = sourceClosureFor(offer);
    const label = closed === 'complete' ? 'closed for every recorded gap' : 'not closed';
    lines.push(`[${label}] ${offer.component} ${offer.version ?? ''} (${offer.license})`.trim());
    if (offer.rebuild) lines.push(`rebuild: ${offer.rebuild}`);
    if (offer.note) lines.push(offer.note);
    if (offer.reason) lines.push(offer.reason);
    for (const archive of offer.archives ?? []) {
      lines.push(`archive: ${archive.name}`);
      lines.push(`sha256: ${archive.sha256}`);
      lines.push(`url: ${archive.url}`);
    }
    lines.push('');
  }
  return `${lines.join('\n')}\n`;
}

async function fetchBytes(url, fetchImpl) {
  const response = await fetchImpl(url);
  if (!response.ok) {
    throw new Error(`GET ${url} failed: ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

function kitFromDir(outDir, names) {
  const kit = {};
  for (const name of names) {
    const file = path.join(outDir, name);
    if (!fs.existsSync(file)) continue;
    const bytes = fs.readFileSync(file);
    kit[name] = { sha256: sha256(bytes) };
  }
  return kit;
}

function readAssets(assetsDir) {
  const manifests = [];
  const intelEntries = {};
  if (!assetsDir || !fs.existsSync(assetsDir)) return { manifests, intelEntries };
  for (const name of fs.readdirSync(assetsDir)) {
    if (!name.endsWith('.manifest.json')) continue;
    const manifest = JSON.parse(fs.readFileSync(path.join(assetsDir, name), 'utf8'));
    manifests.push(manifest);
    if (!manifest.target) continue;
    const prefix = name.slice(0, -'.manifest.json'.length);
    const tarPath = path.join(assetsDir, `${prefix}.mpv-corresponding-source.tar.gz`);
    intelEntries[manifest.target] = fs.existsSync(tarPath) ? listTarEntries(tarPath) : [];
  }
  return { manifests, intelEntries };
}

function defaultPlans() {
  return Object.fromEntries(RELEASE_TARGETS.map((target) => [target, planFor(target)]));
}

/**
 * @param {string} outDir
 * @param {{
 *   offers?: typeof sourceOffers.offers,
 *   fetch?: typeof fetch,
 *   mode?: 'prepare' | 'publish',
 *   plans?: Record<string, object>,
 *   assetsDir?: string,
 *   manifests?: object[],
 *   intelEntries?: Record<string, string[]>,
 * }} [options]
 */
export async function stageSourceOffers(outDir, options = {}) {
  const mode = options.mode ?? 'prepare';
  const offers = options.offers ?? sourceOffers.offers;
  const fetchImpl = options.fetch ?? fetch;
  const plans = options.plans ?? defaultPlans();
  fs.mkdirSync(outDir, { recursive: true });

  for (const source of pinnedSources(offers)) {
    const dest = path.join(outDir, source.name);
    if (fs.existsSync(dest)) {
      const existing = fs.readFileSync(dest);
      if (sha256(existing) === source.sha256) continue;
    }
    const bytes = await fetchBytes(source.url, fetchImpl);
    assertSha256(bytes, source.sha256, source.name);
    fs.writeFileSync(dest, bytes);
  }

  for (const offer of offers) {
    if (offer.dependencies?.length) {
      const build = (offer.archives ?? []).find((archive) =>
        archive.name.startsWith('FFmpeg-Builds-'),
      );
      if (build) {
        assertBtbnPins(path.join(outDir, build.name), offer.dependencies, {
          branchFile: 'addins/9.0.sh',
          branch: offer.ffmpegBranch,
        });
      }
    }
    if (offer.ffmpegCommit) {
      const ffmpeg = (offer.archives ?? []).find((archive) =>
        archive.name.includes(offer.ffmpegCommit),
      );
      if (ffmpeg) assertFfmpegCommitArchive(path.join(outDir, ffmpeg.name), offer.ffmpegCommit);
    }
    for (const dep of [...(offer.dependencies ?? []), ...(offer.bundledSources ?? [])]) {
      if (!dep.source?.licensePath || !dep.source?.name) continue;
      const file = path.join(outDir, dep.source.name);
      if (!fs.existsSync(file)) continue;
      if (!tarHasEntry(file, dep.source.licensePath)) {
        throw new Error(`${dep.source.name} does not contain ${dep.source.licensePath}`);
      }
    }
  }

  const names = [...new Set(pinnedSources(offers).map((source) => source.name))];
  const kit = kitFromDir(outDir, names);
  const fromAssets = readAssets(options.assetsDir);
  const manifests = options.manifests ?? fromAssets.manifests;
  const intelEntries = options.intelEntries ?? fromAssets.intelEntries;
  if (mode === 'publish' && manifests.length === 0) {
    throw new Error('publish mode requires component manifests');
  }
  const blockers = publicationBlockers({ offers, plans, manifests, kit, intelEntries });
  fs.copyFileSync(
    new URL('./upstream-source-offers.json', import.meta.url),
    path.join(outDir, 'upstream-source-offers.json'),
  );
  fs.writeFileSync(path.join(outDir, 'SOURCE-OFFER.txt'), offerText(offers));
  fs.writeFileSync(
    path.join(outDir, 'SOURCE-KIT-STATUS.json'),
    `${JSON.stringify({ mode, closed: blockers.length === 0, blockers }, null, 2)}\n`,
  );
  if (mode === 'publish' && blockers.length) {
    const sample = blockers
      .slice(0, 8)
      .map((blocker) => `${blocker.code}:${blocker.detail ?? blocker.component}`)
      .join(', ');
    throw new Error(`corresponding source is not closed (${blockers.length}): ${sample}`);
  }
  return blockers;
}

function parseArgs(argv) {
  const args = { mode: 'prepare', assetsDir: undefined, outDir: undefined };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === '--mode') {
      args.mode = argv[i + 1];
      i += 1;
    } else if (arg === '--assets') {
      args.assetsDir = argv[i + 1];
      i += 1;
    } else if (!arg.startsWith('--') && !args.outDir) {
      args.outDir = arg;
    }
  }
  return args;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = parseArgs(process.argv.slice(2));
  if (!args.outDir || (args.mode !== 'prepare' && args.mode !== 'publish')) {
    console.error(
      'usage: stage-source-offers.mjs [--mode prepare|publish] [--assets <release-assets>] <output-directory>',
    );
    process.exit(1);
  }
  stageSourceOffers(args.outDir, { mode: args.mode, assetsDir: args.assetsDir }).catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
