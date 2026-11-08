#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { createReadStream } from 'node:fs';
import { mkdtemp, rm } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { planFor } from '../sidecars/plan.mjs';
import { publicationBlockers } from './source-kit-gate.mjs';
import sourceOffers from './upstream-source-offers.json' with { type: 'json' };
import {
  REQUIRED_TARGETS,
  REUSE_REPO,
  loadReuseDocument,
  packageNameFromZipListing,
  provenanceNotes,
  publicationDecision,
  verifyReuseSet,
} from './reused-artifacts.mjs';

function gh(args) {
  const result = spawnSync('gh', args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error((result.stderr || result.stdout || 'gh failed').trim());
  }
  return result.stdout;
}

function sha256File(file) {
  return new Promise((resolve, reject) => {
    const hash = createHash('sha256');
    createReadStream(file)
      .on('data', (chunk) => hash.update(chunk))
      .on('error', reject)
      .on('end', () => resolve(hash.digest('hex')));
  });
}

function zipNames(zip) {
  const result = spawnSync('unzip', ['-Z1', zip], { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(result.stderr || `cannot list ${zip}`);
  return result.stdout.split('\n').filter(Boolean);
}

async function observe(doc) {
  if (doc.repo !== REUSE_REPO) {
    throw new Error(`refusing to download artifacts from ${doc.repo}`);
  }
  const observations = {};
  for (const target of REQUIRED_TARGETS) {
    const pin = doc.targets?.[target];
    if (!pin || pin.status !== 'pinned') continue;
    const meta = JSON.parse(gh(['api', `repos/${doc.repo}/actions/artifacts/${pin.artifactId}`]));
    const dir = await mkdtemp(path.join(os.tmpdir(), 'vitra-reuse-'));
    const zip = path.join(dir, 'artifact.zip');
    try {
      gh(['api', `repos/${doc.repo}/actions/artifacts/${pin.artifactId}/zip`, '--output', zip]);
      observations[target] = {
        repo: doc.repo,
        runId: meta.workflow_run?.id,
        headSha: meta.workflow_run?.head_sha,
        artifactId: meta.id,
        artifactName: meta.name,
        size: meta.size_in_bytes,
        expired: meta.expired,
        sha256: await sha256File(zip),
        packageName: packageNameFromZipListing(zipNames(zip)),
      };
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  }
  return observations;
}

function sourceBlockers() {
  const plans = Object.fromEntries(REQUIRED_TARGETS.map((target) => [target, planFor(target)]));
  return publicationBlockers({ offers: sourceOffers.offers, plans });
}

export function reportDecision(decision) {
  for (const error of decision.reuseErrors) {
    console.error(`${error.code}\t${error.target ?? ''}\t${error.detail ?? ''}`);
  }
  for (const blocker of decision.blockers) {
    console.error(`${blocker.code}\t${blocker.target ?? ''}\t${blocker.detail ?? ''}`);
  }
  return decision.ok;
}

async function main() {
  const doc = loadReuseDocument();
  const observations = await observe(doc);
  const decision = publicationDecision(verifyReuseSet(doc, observations), sourceBlockers());
  if (!reportDecision(decision)) process.exit(1);
  console.log(provenanceNotes(doc));
  if (process.argv.includes('--publish')) {
    console.error('refusing to create a GitHub Release from this command');
    process.exit(1);
  }
  console.log('pinned bytes match; not publishing');
}

const isMain = process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url);
if (isMain) {
  main().catch((error) => {
    console.error(error.message);
    process.exit(1);
  });
}
