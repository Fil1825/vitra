import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import { planFor } from '../sidecars/plan.mjs';
import { publicationBlockers, sourceRecipeFiles } from './source-kit-gate.mjs';
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
      ...(pin.sourceTar
        ? {
            sourceTarSha256: pin.sourceTar.sha256,
            sourceTarEntries: pin.sourceTar.entries,
          }
        : {}),
      ...patch[target],
    };
  }
  return observations;
}

function withPinned(doc) {
  const targets = {};
  for (const target of REQUIRED_TARGETS) {
    const current = doc.targets[target];
    const files = [
      ...new Set(
        Object.values(planFor(target))
          .filter((sidecar) => sidecar.type === 'source-build')
          .flatMap((sidecar) => sourceRecipeFiles(sidecar, target)),
      ),
    ];
    targets[target] = {
      ...current,
      status: 'pinned',
      sourceTar: {
        sha256: current.sourceTar?.sha256 ?? 'b'.repeat(64),
        entries: files.map((file) => `mpv-corresponding-source/${file}`),
      },
    };
  }
  return { ...doc, targets };
}

describe('reused installer pins', () => {
  const doc = loadReuseDocument();

  it('records the four downloaded artifact zips', () => {
    expect(doc.repo).toBe('Fil1825/vitra');
    expect(doc.version).toBe('1.0.0-rc.1');
    expect(doc.appPayloadCommit).toBe('76ea04250e88d880c3b6a7e3abf182f29c6bbde2');
    for (const [target, downloaded] of Object.entries(DOWNLOADED)) {
      const pin = doc.targets[target];
      expect(pin.status).toBe('superseded');
      expect(pin.runId).toBe(42512576166);
      expect(pin.headSha).toBe(doc.appPayloadCommit);
      expect(pin.artifactId).toBe(downloaded.artifactId);
      expect(pin.size).toBe(downloaded.size);
      expect(pin.sha256).toBe(downloaded.sha256);
      expect(pin.packageName).toBe(downloaded.packageName);
    }
    const intel = doc.targets['x86_64-apple-darwin'];
    expect(intel.status).toBe('superseded');
    expect(intel.runId).toBe(43638348270);
    expect(intel.headSha).toBe('2adf1a5499f6a6c210ee4c29a8dcebd3c54ae156');
    expect(intel.artifactId).toBe(14132604382);
    expect(intel.sha256).toBe('573ea7b5ed750e2e4d44d106f77e553eac5664575143ac4dc7fb6ee4878551a6');
    expect(intel.sourceTar.sha256).toBe(
      'cbaafbcb06057c9e7f2fdeb2c71d29a8521a25010b0148d86379337afdcfc736',
    );
    expect(intel.expiresAt).toBe('2026-11-28T21:30:33Z');
    expect(doc.targets['x86_64-unknown-linux-gnu'].expiresAt).toBe('2026-11-22T19:05:50Z');
  });

  it('accepts a complete pin and rejects the superseded installers', () => {
    const pinned = withPinned(doc);
    expect(verifyReuseSet(pinned, observationsFrom(pinned))).toEqual([]);
    expect(verifyReuseSet(doc, observationsFrom(doc)).map((error) => error.code)).toEqual(
      REQUIRED_TARGETS.map(() => 'missing-target'),
    );
    const wrongTar = observationsFrom(pinned, {
      'x86_64-apple-darwin': { sourceTarSha256: 'd'.repeat(64) },
    });
    expect(
      verifyReuseSet(pinned, wrongTar).some((error) => error.code === 'wrong-source-tar'),
    ).toBe(true);
  });

  it('rejects the wrong repository, run, commit, artifact, or version', () => {
    const complete = withPinned(doc);
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
    const complete = withPinned(doc);
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
    expect(notes).toContain('x86_64-unknown-linux-gnu: not pinned');
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

  it('does not ship Martin Riedl, official mpv, or the seven unpinned libraries', () => {
    const plans = Object.fromEntries(REQUIRED_TARGETS.map((target) => [target, planFor(target)]));
    const blockers = publicationBlockers({ offers: sourceOffers.offers, plans });
    const details = new Set(blockers.map((blocker) => blocker.detail));
    expect(details.has('martin-riedl-macos-ffmpeg')).toBe(false);
    expect(details.has('mpv-official-v0.41.0')).toBe(false);
    expect(details.has('GNU Readline')).toBe(false);
    expect(details.has('libintl')).toBe(false);
    expect(details.has('libidn2')).toBe(false);
    expect(details.has('libunistring')).toBe(false);
    expect(details.has('50-aom.sh')).toBe(false);
    for (const offer of sourceOffers.offers) expect(offer.complete).toBe(false);
  });
});

describe('release workflow launch', () => {
  const yaml = fs.readFileSync(path.join(root, '.github/workflows/release.yml'), 'utf8');

  it('rebuilds the four installers from a pull request and does not rebuild on main', () => {
    expect(yaml).not.toMatch(/branches:\s*\[main\]/);
    expect(yaml).not.toContain('pull_request_target');
    expect(yaml).toContain('pull_request:');
    expect(yaml).toContain('macos-15-intel');
    expect(yaml).toContain('macos-15');
    expect(yaml).toContain('windows-2022');
    expect(yaml).toContain('ubuntu-24.04');
    expect(yaml).not.toContain('app-payload.mjs');
    const pinned = yaml.slice(yaml.indexOf('publish-pinned:'));
    expect(pinned).not.toContain('needs:');
    expect(pinned).not.toContain('gh release create');
  });
});
