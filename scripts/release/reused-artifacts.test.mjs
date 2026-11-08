import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { planFor } from '../sidecars/plan.mjs';
import { publicationBlockers } from './source-kit-gate.mjs';
import sourceOffers from './upstream-source-offers.json' with { type: 'json' };
import {
  REQUIRED_TARGETS,
  loadReuseDocument,
  packageNameFromZipListing,
  provenanceNotes,
  publicationDecision,
  verifyReuseSet,
} from './reused-artifacts.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const DOWNLOADED = {
  'x86_64-unknown-linux-gnu': {
    artifactId: 13629864717,
    size: 232932785,
    sha256: '6a1aa256b992c820b2f9ec35c8b3858db69bf02ec2eb2bc6f0211cb42f00d332',
    packageName: 'Vitra_1.0.0-rc.1_amd64.deb',
  },
  'x86_64-pc-windows-msvc': {
    artifactId: 13630826011,
    size: 171959682,
    sha256: '5fa00d1ae113eef2617b2f7e6649df0761fabd3e19c157d41d3abb5f80c36893',
    packageName: 'Vitra_1.0.0-rc.1_x64-setup.exe',
  },
  'aarch64-apple-darwin': {
    artifactId: 13630098011,
    size: 196595124,
    sha256: '243c15227b4ed535b52da8e701471aa455637dda3ef495e032598efc9230337e',
    packageName: 'Vitra_1.0.0-rc.1_aarch64.dmg',
  },
};

function observationsFrom(doc, patch = {}) {
  const observations = {};
  for (const [target, pin] of Object.entries(doc.targets)) {
    if (pin.status !== 'pinned') continue;
    observations[target] = {
      repo: doc.repo,
      runId: pin.runId,
      headSha: pin.headSha,
      artifactId: pin.artifactId,
      artifactName: pin.artifactName,
      size: pin.size,
      expired: false,
      sha256: pin.sha256,
      packageName: pin.packageName,
      ...patch[target],
    };
  }
  return observations;
}

function withIntel(doc) {
  return {
    ...doc,
    targets: {
      ...doc.targets,
      'x86_64-apple-darwin': {
        status: 'pinned',
        runId: 9,
        headSha: 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
        artifactId: 1,
        artifactName: 'Vitra-macOS-Intel',
        size: 10,
        sha256: 'b'.repeat(64),
        packageName: 'Vitra_1.0.0-rc.1_x64.dmg',
        arch: 'x64',
      },
    },
  };
}

describe('reused installer pins', () => {
  const doc = loadReuseDocument();

  it('records the three downloaded artifact zips and leaves Intel unpinned', () => {
    expect(doc.repo).toBe('Fil1825/vitra');
    expect(doc.version).toBe('1.0.0-rc.1');
    expect(doc.appPayloadCommit).toBe('76ea04250e88d880c3b6a7e3abf182f29c6bbde2');
    for (const [target, downloaded] of Object.entries(DOWNLOADED)) {
      const pin = doc.targets[target];
      expect(pin.status).toBe('pinned');
      expect(pin.runId).toBe(42512576166);
      expect(pin.headSha).toBe(doc.appPayloadCommit);
      expect(pin.artifactId).toBe(downloaded.artifactId);
      expect(pin.size).toBe(downloaded.size);
      expect(pin.sha256).toBe(downloaded.sha256);
      expect(pin.packageName).toBe(downloaded.packageName);
    }
    expect(doc.targets['x86_64-apple-darwin'].status).toBe('missing');
  });

  it('accepts a complete set and rejects a missing Intel pin', () => {
    const complete = withIntel(doc);
    expect(verifyReuseSet(complete, observationsFrom(complete))).toEqual([]);
    const errors = verifyReuseSet(doc, observationsFrom(doc));
    expect(errors).toEqual([{ code: 'missing-target', target: 'x86_64-apple-darwin' }]);
  });

  it('rejects the wrong repository, run, commit, artifact, or version', () => {
    const complete = withIntel(doc);
    const cases = [
      [{ repo: 'other/vitra' }, 'wrong-repo'],
      [{ runId: 1 }, 'wrong-run'],
      [{ headSha: 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' }, 'wrong-head'],
      [{ artifactId: 2, artifactName: 'Vitra-other' }, 'wrong-artifact'],
      [{ packageName: 'Vitra_9.9.9_amd64.deb' }, 'wrong-package'],
    ];
    for (const [patch, code] of cases) {
      const observed = observationsFrom(complete, { 'x86_64-unknown-linux-gnu': patch });
      expect(verifyReuseSet(complete, observed).some((error) => error.code === code)).toBe(true);
    }
    const wrongVersion = { ...complete, version: '1.0.0' };
    expect(verifyReuseSet(wrongVersion, observationsFrom(complete))[0].code).toBe('wrong-version');
    const renamed = {
      ...complete,
      targets: {
        ...complete.targets,
        'x86_64-unknown-linux-gnu': {
          ...complete.targets['x86_64-unknown-linux-gnu'],
          packageName: 'Vitra_1.0.0_amd64.deb',
        },
      },
    };
    expect(
      verifyReuseSet(renamed, observationsFrom(renamed)).some(
        (error) => error.code === 'wrong-version',
      ),
    ).toBe(true);
  });

  it('rejects an expired zip and a zip whose bytes were substituted', () => {
    const complete = withIntel(doc);
    const expired = observationsFrom(complete, { 'aarch64-apple-darwin': { expired: true } });
    expect(verifyReuseSet(complete, expired).some((error) => error.code === 'expired')).toBe(true);
    const substituted = observationsFrom(complete, {
      'x86_64-pc-windows-msvc': { sha256: 'c'.repeat(64), size: 1 },
    });
    const codes = verifyReuseSet(complete, substituted).map((error) => error.code);
    expect(codes).toContain('wrong-sha');
    expect(codes).toContain('substituted');
  });

  it('reads the single installer name out of an artifact zip listing', () => {
    expect(
      packageNameFromZipListing([
        'target/x86_64-unknown-linux-gnu/release/bundle/deb/Vitra_1.0.0-rc.1_amd64.deb',
        'src-tauri/sidecars/x86_64-unknown-linux-gnu/manifest.json',
      ]),
    ).toBe('Vitra_1.0.0-rc.1_amd64.deb');
    expect(packageNameFromZipListing(['a.deb', 'b.deb'])).toBeNull();
  });

  it('states that a release commit is not the build commit of every file', () => {
    const notes = provenanceNotes(doc);
    expect(notes).toContain('were not all built from one commit');
    expect(notes).toContain(DOWNLOADED['x86_64-unknown-linux-gnu'].sha256);
    expect(notes).toContain('x86_64-apple-darwin: not pinned');
  });

  it('does not treat a skipped build matrix or an open source gate as publication', () => {
    expect(publicationDecision([{ code: 'missing-target' }], [])).toMatchObject({ ok: false });
    expect(
      publicationDecision(
        [],
        [{ code: 'uncovered-shipped-component', detail: 'martin-riedl-macos-ffmpeg' }],
      ),
    ).toMatchObject({ ok: false });
    expect(publicationDecision([], [])).toMatchObject({ ok: true });
  });

  it('keeps Martin Riedl, official mpv, and the unpinned yt-dlp libraries open', () => {
    const plans = Object.fromEntries(REQUIRED_TARGETS.map((target) => [target, planFor(target)]));
    const blockers = publicationBlockers({ offers: sourceOffers.offers, plans });
    const details = new Set(blockers.map((blocker) => blocker.detail));
    expect(details.has('martin-riedl-macos-ffmpeg')).toBe(true);
    expect(details.has('mpv-official-v0.41.0')).toBe(true);
    expect(details.has('GNU Readline')).toBe(true);
    expect(details.has('libintl')).toBe(true);
    expect(details.has('libidn2')).toBe(true);
    expect(details.has('libunistring')).toBe(true);
    for (const offer of sourceOffers.offers) expect(offer.complete).toBe(false);
  });
});

describe('release workflow launch', () => {
  const yaml = fs.readFileSync(path.join(root, '.github/workflows/release.yml'), 'utf8');

  it('starts only the Intel package from a pull request and does not rebuild on main', () => {
    expect(yaml).not.toMatch(/branches:\s*\[main\]/);
    expect(yaml).not.toContain('pull_request_target');
    expect(yaml).toContain('pull_request:');
    expect(yaml).toContain("matrix.name == 'macOS-Intel'");
    expect(yaml).toContain('macos-15-intel');
    expect(yaml).toContain('app-payload.mjs');
    expect(yaml).toContain(loadReuseDocument().appPayloadCommit);
    const pinned = yaml.slice(yaml.indexOf('publish-pinned:'));
    expect(pinned).not.toContain('needs:');
    expect(pinned).not.toContain('gh release create');
  });
});
