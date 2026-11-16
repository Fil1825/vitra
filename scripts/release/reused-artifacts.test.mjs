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

const BUILD = '1307519bc4ad82870c01e7821cbb3afb070b8557';
const RUN = 43772604652;
const RETIRED = [13629864717, 13630826011, 13630098011, 14132604382];

const DOWNLOADED = {
  'x86_64-unknown-linux-gnu': {
    artifactId: 14185725871,
    artifactName: 'Vitra-Linux-x64',
    size: 150903684,
    sha256: '491632c18f54bcf2819d643cffb10d487b5fc38e17fc1ba74b5843d24da8e3c5',
    packageName: 'Vitra_1.0.0-rc.1_amd64.deb',
    expiresAt: '2026-11-29T23:16:34Z',
    sourceTarSha256: '7390728aa5651a83105d7c95feeb8fa3a9c8210e637e7f098e738d31099a0b08',
  },
  'x86_64-pc-windows-msvc': {
    artifactId: 14187285439,
    artifactName: 'Vitra-Windows-x64',
    size: 238525172,
    sha256: 'a7945292c093de0b41f5ab9e683c36a0ce0f2d0523564992b472b4b211721c3d',
    packageName: 'Vitra_1.0.0-rc.1_x64-setup.exe',
    expiresAt: '2026-11-29T23:55:30Z',
    sourceTarSha256: 'a5ec34464980c9c46232df22ed20570ca84357c9b8504b47dcd680b780d8bdd5',
  },
  'aarch64-apple-darwin': {
    artifactId: 14186070710,
    artifactName: 'Vitra-macOS-Apple-Silicon',
    size: 255289984,
    sha256: '6081eedc792330dbaa96ca4c35fe7c51f1a495c87b0862bc5072760a95fb46cf',
    packageName: 'Vitra_1.0.0-rc.1_aarch64.dmg',
    expiresAt: '2026-11-29T23:26:43Z',
    sourceTarSha256: 'f2378703cb0f2fcafa0089060707a624841f9d993fb77aa2026f57a66a1f22fb',
  },
  'x86_64-apple-darwin': {
    artifactId: 14185737506,
    artifactName: 'Vitra-macOS-Intel',
    size: 264963601,
    sha256: 'f8410a6398ecea712d24987a73d19c2c2368b07f371efbde256e53b53ed2f16d',
    packageName: 'Vitra_1.0.0-rc.1_x64.dmg',
    expiresAt: '2026-11-29T23:28:08Z',
    sourceTarSha256: '3fbbbeb3266cc7a4baa42512bea1cbf82acd1f65fb85a44d71a78694e1ab515a',
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

  it('records the four source-built artifact zips and supersedes them', () => {
    expect(doc.repo).toBe('Fil1825/vitra');
    expect(doc.version).toBe('1.0.0-rc.1');
    expect(doc.appPayloadCommit).toBe(BUILD);
    const recorded = JSON.stringify(doc);
    for (const id of RETIRED) expect(recorded).not.toContain(String(id));
    expect(recorded).not.toContain('Martin Riedl');
    expect(recorded).not.toContain('BtbN');
    for (const [target, downloaded] of Object.entries(DOWNLOADED)) {
      const pin = doc.targets[target];
      expect(pin.status).toBe('superseded');
      expect(pin.runId).toBe(RUN);
      expect(pin.headSha).toBe(BUILD);
      expect(pin.artifactId).toBe(downloaded.artifactId);
      expect(pin.artifactName).toBe(downloaded.artifactName);
      expect(pin.size).toBe(downloaded.size);
      expect(pin.sha256).toBe(downloaded.sha256);
      expect(pin.packageName).toBe(downloaded.packageName);
      expect(pin.expiresAt).toBe(downloaded.expiresAt);
      expect(pin.sourceTar.sha256).toBe(downloaded.sourceTarSha256);
    }
  });

  it('accepts the pinned installers and rejects a superseded target', () => {
    expect(verifyReuseSet(doc, observationsFrom(doc)).map((error) => error.code)).toEqual([
      'missing-target',
      'missing-target',
      'missing-target',
      'missing-target',
    ]);
    const complete = withPinned(doc);
    expect(verifyReuseSet(complete, observationsFrom(complete))).toEqual([]);
    const superseded = {
      ...complete,
      targets: {
        ...complete.targets,
        'x86_64-unknown-linux-gnu': {
          ...complete.targets['x86_64-unknown-linux-gnu'],
          status: 'superseded',
        },
      },
    };
    expect(
      verifyReuseSet(superseded, observationsFrom(superseded)).map((error) => error.code),
    ).toEqual(['missing-target']);
    const wrongTar = observationsFrom(complete, {
      'x86_64-apple-darwin': { sourceTarSha256: 'd'.repeat(64) },
    });
    expect(
      verifyReuseSet(complete, wrongTar).some((error) => error.code === 'wrong-source-tar'),
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
    const live = provenanceNotes(doc);
    expect(live).toContain('were not all built from one commit');
    expect(live).toContain('- x86_64-unknown-linux-gnu: not pinned');
    const notes = provenanceNotes(withPinned(doc));
    expect(notes).toContain(`were built from commit ${BUILD}`);
    expect(notes).toContain('is not the build commit of every file');
    expect(notes).toContain('artifact 14185725871 Vitra-Linux-x64');
    expect(notes).not.toContain('not pinned');
    const pinned = withPinned(doc);
    const mixed = provenanceNotes({
      ...pinned,
      targets: {
        ...pinned.targets,
        'x86_64-apple-darwin': {
          ...pinned.targets['x86_64-apple-darwin'],
          headSha: 'a'.repeat(40),
        },
      },
    });
    expect(mixed).toContain('were not all built from one commit');
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
