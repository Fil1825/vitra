// Download the complete corresponding-source archives named in
// upstream-source-offers.json, reject a hash or BtbN pin mismatch, and write
// a directory a release can ship next to the installers.
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

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

function tarEntries(archive) {
  const result = spawnSync('tar', ['-tzf', archive], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(result.stderr || `cannot list ${archive}`);
  }
  return result.stdout.split('\n').filter(Boolean);
}

function tarText(archive, entry) {
  const result = spawnSync('tar', ['-xOzf', archive, entry], { encoding: 'utf8' });
  if (result.status !== 0) {
    throw new Error(`cannot read ${entry} from ${archive}`);
  }
  return result.stdout;
}

/** Dependency pins declared by a BtbN FFmpeg-Builds tag archive. */
export function pinsFromBtbnArchive(archive) {
  const pins = [];
  for (const entry of tarEntries(archive)) {
    if (!entry.includes('/scripts.d/') || !entry.endsWith('.sh')) continue;
    const text = tarText(archive, entry);
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

function pinKey(pin) {
  return `${pin.script}\0${pin.repo ?? ''}\0${pin.commit ?? ''}\0${pin.svn ?? ''}`;
}

/** Throws when the archive's pins differ from the offer manifest. */
export function assertBtbnPins(archive, expected, { branchFile, branch }) {
  const actual = pinsFromBtbnArchive(archive);
  const actualKeys = new Set(actual.map(pinKey));
  const expectedKeys = new Set(expected.map(pinKey));
  const missing = [...expectedKeys].filter((key) => !actualKeys.has(key));
  const extra = [...actualKeys].filter((key) => !expectedKeys.has(key));
  if (missing.length || extra.length) {
    throw new Error(`BtbN pin mismatch: ${missing.length} missing, ${extra.length} unexpected`);
  }
  const branchEntry = tarEntries(archive).find((entry) => entry.endsWith(`/${branchFile}`));
  if (!branchEntry) throw new Error(`BtbN archive has no ${branchFile}`);
  const text = tarText(archive, branchEntry);
  if (!text.includes(`GIT_BRANCH="${branch}"`)) {
    throw new Error(`${branchFile} does not set GIT_BRANCH=${branch}`);
  }
}

function offerText(offers) {
  const lines = [
    'Corresponding source shipped with this Vitra release.',
    'Hashes are SHA-256 of the archives in this directory.',
    '',
  ];
  for (const offer of offers) {
    if (offer.complete) {
      lines.push(`[complete] ${offer.component} ${offer.version} (${offer.license})`);
      if (offer.rebuild) lines.push(`rebuild: ${offer.rebuild}`);
      lines.push(offer.note);
      for (const archive of offer.archives) {
        lines.push(`archive: ${archive.name}`);
        lines.push(`sha256: ${archive.sha256}`);
        lines.push(`url: ${archive.url}`);
      }
      if (offer.dependencies) {
        lines.push(
          `dependency pins: ${offer.dependencies.length} (see upstream-source-offers.json)`,
        );
      }
    } else {
      lines.push(`[not complete] ${offer.component} (${offer.license})`);
      lines.push(offer.reason);
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

/**
 * @param {string} outDir
 * @param {{ offers?: typeof sourceOffers.offers, fetch?: typeof fetch }} [options]
 */
export async function stageSourceOffers(outDir, options = {}) {
  const offers = options.offers ?? sourceOffers.offers;
  const fetchImpl = options.fetch ?? fetch;
  fs.mkdirSync(outDir, { recursive: true });
  for (const offer of offers) {
    if (!offer.complete) continue;
    for (const archive of offer.archives) {
      const bytes = await fetchBytes(archive.url, fetchImpl);
      assertSha256(bytes, archive.sha256, archive.name);
      fs.writeFileSync(path.join(outDir, archive.name), bytes);
    }
    if (offer.dependencies) {
      const build = offer.archives.find((archive) => archive.name.startsWith('FFmpeg-Builds-'));
      assertBtbnPins(path.join(outDir, build.name), offer.dependencies, {
        branchFile: 'addins/9.0.sh',
        branch: offer.ffmpegBranch,
      });
    }
  }
  fs.copyFileSync(
    new URL('./upstream-source-offers.json', import.meta.url),
    path.join(outDir, 'upstream-source-offers.json'),
  );
  fs.writeFileSync(path.join(outDir, 'SOURCE-OFFER.txt'), offerText(offers));
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const outDir = process.argv[2];
  if (!outDir) {
    console.error('usage: stage-source-offers.mjs <output-directory>');
    process.exit(1);
  }
  stageSourceOffers(outDir).catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
}
