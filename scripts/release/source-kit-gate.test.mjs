import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { planFor } from '../sidecars/plan.mjs';
import {
  INTEL_SOURCE_FILES,
  RELEASE_TARGETS,
  publicationBlockers,
  sourceClosureFor,
} from './source-kit-gate.mjs';
import sourceOffers from './upstream-source-offers.json' with { type: 'json' };
import { listTarEntries, sha256, stageSourceOffers } from './stage-source-offers.mjs';

const COMMIT = 'd4070a14cecc7e29eb278f9b6036c2284255f70d';
const ASSET = 'ffmpeg-n9.0.2-48-gd4070a14ce-win64-gpl-9.0.zip';
const X264 = {
  script: '50-x264.sh',
  repo: 'https://code.videolan.org/videolan/x264.git',
  commit: '0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee',
};

function fixtureArchive(rootName, files) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-offer-'));
  const root = path.join(dir, rootName);
  for (const [name, text] of Object.entries(files)) {
    const file = path.join(root, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
  }
  const archive = path.join(dir, `${rootName}.tar.gz`);
  const packed = spawnSync('tar', ['-czf', archive, '-C', dir, rootName]);
  if (packed.status !== 0) throw new Error(packed.stderr.toString());
  return archive;
}

function bytesOf(archive) {
  return fs.readFileSync(archive);
}

function closedFixture() {
  const builds = bytesOf(
    fixtureArchive('FFmpeg-Builds-fixture', {
      'scripts.d/50-x264.sh': `SCRIPT_REPO="${X264.repo}"\nSCRIPT_COMMIT="${X264.commit}"\n`,
      'scripts.d/15-base.sh': 'SCRIPT_SKIP="1"\n',
      'addins/9.0.sh': 'GIT_BRANCH="release/9.0"\n',
    }),
  );
  const ffmpeg = bytesOf(fixtureArchive(`FFmpeg-${COMMIT}`, { configure: 'configured\n' }));
  const x264 = bytesOf(fixtureArchive('x264', { LICENSE: 'GNU GENERAL PUBLIC LICENSE\n' }));
  const offer = {
    id: 'fixture-btbn',
    component: 'ffmpeg',
    version: 'fixture',
    license: 'GPL-3.0-or-later',
    complete: true,
    ffmpegCommit: COMMIT,
    ffmpegBranch: 'release/9.0',
    targets: ['win64'],
    archives: [
      {
        name: `FFmpeg-${COMMIT}.tar.gz`,
        url: 'https://example.test/ffmpeg.tar.gz',
        sha256: sha256(ffmpeg),
      },
      {
        name: 'FFmpeg-Builds-fixture.tar.gz',
        url: 'https://example.test/builds.tar.gz',
        sha256: sha256(builds),
      },
    ],
    dependencies: [
      {
        ...X264,
        shipsOn: ['win64'],
        source: {
          name: 'x264.tar.gz',
          url: 'https://example.test/x264.tar.gz',
          sha256: sha256(x264),
          licensePath: 'x264/LICENSE',
        },
      },
    ],
  };
  const bodies = new Map([
    [offer.archives[0].url, ffmpeg],
    [offer.archives[1].url, builds],
    [offer.dependencies[0].source.url, x264],
  ]);
  const plans = {
    'x86_64-pc-windows-msvc': {
      ffmpeg: { type: 'download', downloads: [{ name: ASSET, sourceOfferId: offer.id }] },
    },
  };
  const manifests = [
    {
      target: 'x86_64-pc-windows-msvc',
      sidecars: { ffmpeg: { downloads: [{ name: ASSET }] } },
    },
  ];
  return { offer, bodies, plans, manifests };
}

describe('publication gate', () => {
  it('accepts a kit whose archives, license file, and FFmpeg commit all match', async () => {
    const { offer, bodies, plans, manifests } = closedFixture();
    const out = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-gate-'));
    const fetchImpl = async (url) => ({
      ok: true,
      arrayBuffer: async () => bodies.get(url),
    });
    const prepare = await stageSourceOffers(out, {
      mode: 'prepare',
      offers: [offer],
      plans,
      manifests,
      fetch: fetchImpl,
    });
    expect(prepare).toEqual([]);
    expect(listTarEntries(path.join(out, 'x264.tar.gz'))).toContain('x264/LICENSE');
    expect(sourceClosureFor(offer, { os: 'windows', ffmpegKey: 'win64' })).toBe('complete');
    await expect(
      stageSourceOffers(out, {
        mode: 'publish',
        offers: [offer],
        plans,
        manifests,
        fetch: fetchImpl,
      }),
    ).resolves.toEqual([]);
  });

  it('still blocks publish when complete is true but a dependency archive is missing', async () => {
    const { offer, bodies, plans, manifests } = closedFixture();
    delete offer.dependencies[0].source;
    const out = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-gate-'));
    const options = {
      offers: [offer],
      plans,
      manifests,
      fetch: async (url) => ({ ok: true, arrayBuffer: async () => bodies.get(url) }),
    };
    const blockers = await stageSourceOffers(out, { ...options, mode: 'prepare' });
    expect(blockers.some((blocker) => blocker.detail === '50-x264.sh')).toBe(true);
    await expect(stageSourceOffers(out, { ...options, mode: 'publish' })).rejects.toThrow(
      /not closed/,
    );
  });

  it('blocks the real release plans on known open components', () => {
    const plans = Object.fromEntries(RELEASE_TARGETS.map((target) => [target, planFor(target)]));
    const blockers = publicationBlockers({ offers: sourceOffers.offers, plans });
    const details = (code, target) =>
      blockers.filter((blocker) => blocker.code === code && (!target || blocker.target === target));

    expect(
      details('uncovered-shipped-component').some((b) => b.detail === 'martin-riedl-macos-ffmpeg'),
    ).toBe(true);
    expect(
      details('uncovered-shipped-component').some((b) => b.detail === 'mpv-official-v0.41.0'),
    ).toBe(true);
    expect(
      details('missing-dependency-source', 'x86_64-pc-windows-msvc').some(
        (b) => b.detail === '50-x264.sh',
      ),
    ).toBe(true);
    expect(
      details('missing-dependency-source', 'x86_64-unknown-linux-gnu').some(
        (b) => b.detail === '10-mingw.sh',
      ),
    ).toBe(false);
    expect(
      details('missing-dependency-source', 'x86_64-pc-windows-msvc').some(
        (b) => b.detail === '10-mingw.sh',
      ),
    ).toBe(true);
    expect(blockers.some((b) => b.detail === '50-fdk-aac.sh' || b.detail === '50-whisper.sh')).toBe(
      false,
    );
    expect(
      details('missing-dependency-source', 'x86_64-unknown-linux-gnu').some(
        (b) => b.detail === 'GNU Readline',
      ),
    ).toBe(true);
    expect(
      details('missing-dependency-source', 'x86_64-pc-windows-msvc').some(
        (b) => b.detail === 'GNU Readline',
      ),
    ).toBe(false);
    expect(
      blockers.some((b) => b.component === 'deno' || /libstdc|libgcc/.test(b.detail ?? '')),
    ).toBe(false);
    expect(
      blockers.some((b) => b.target === 'x86_64-unknown-linux-gnu' && b.component === 'mpv'),
    ).toBe(false);

    const ytdlp = sourceOffers.offers.find((offer) => offer.id === 'yt-dlp-2026.10.26');
    expect(sourceClosureFor(ytdlp, { os: 'windows' })).toBe('complete');
    expect(sourceClosureFor(ytdlp, { os: 'linux' })).toBe('incomplete');
    expect(planFor('x86_64-pc-windows-msvc').ffmpeg.downloads[0].sourceClosure).toBe('incomplete');
  });

  it('rejects an FFmpeg asset whose short hash is not that commit', () => {
    const target = 'x86_64-pc-windows-msvc';
    const plan = planFor(target);
    plan.ffmpeg.downloads[0].name = 'ffmpeg-n9.0.2-48-g0000000000-win64-gpl-9.0.zip';
    const blockers = publicationBlockers({
      offers: sourceOffers.offers,
      plans: { [target]: plan },
    });
    expect(blockers.some((blocker) => blocker.code === 'ffmpeg-commit-mismatch')).toBe(true);
    const offer = sourceOffers.offers.find((item) => item.ffmpegCommit);
    const archive = offer.archives.find((item) => item.name.includes(offer.ffmpegCommit));
    const listed = publicationBlockers({
      offers: [offer],
      plans: {
        [target]: {
          ffmpeg: {
            type: 'download',
            downloads: [{ name: ASSET, sourceOfferId: offer.id }],
          },
        },
      },
      kit: { [archive.name]: { sha256: archive.sha256, entries: ['FFmpeg-other/configure'] } },
    });
    expect(
      listed.some(
        (blocker) => blocker.code === 'ffmpeg-commit-mismatch' && blocker.detail === archive.name,
      ),
    ).toBe(true);
  });

  it('rejects a manifest binary the plan does not ship', () => {
    const target = 'x86_64-pc-windows-msvc';
    const blockers = publicationBlockers({
      offers: sourceOffers.offers,
      plans: { [target]: planFor(target) },
      manifests: [{ target, sidecars: { extra: { downloads: [{ name: 'mystery.exe' }] } } }],
    });
    expect(
      blockers.some(
        (blocker) => blocker.code === 'unknown-shipped-binary' && blocker.detail === 'mystery.exe',
      ),
    ).toBe(true);
  });

  it('requires the Intel source archive to contain the recipe files', () => {
    const target = 'x86_64-apple-darwin';
    const archive = fixtureArchive('mpv-corresponding-source', {
      'build-intel-mpv.sh': '#!/bin/sh\n',
      'intel-mpv-sources.json': '{}\n',
      'mach-minos.py': 'print(1)\n',
      'mach-deps.py': 'print(1)\n',
    });
    const entries = listTarEntries(archive);
    for (const file of INTEL_SOURCE_FILES) {
      expect(entries.some((entry) => entry.endsWith(file))).toBe(true);
    }
    const plans = { [target]: planFor(target) };
    const manifests = [{ target, sidecars: {} }];
    expect(
      publicationBlockers({
        offers: sourceOffers.offers,
        plans,
        manifests,
        intelEntries: { [target]: entries },
      }).some((blocker) => blocker.code === 'missing-intel-source'),
    ).toBe(false);
    expect(
      publicationBlockers({
        offers: sourceOffers.offers,
        plans,
        manifests,
        intelEntries: { [target]: entries.filter((entry) => !entry.endsWith('mach-deps.py')) },
      }).some((blocker) => blocker.code === 'missing-intel-source'),
    ).toBe(true);
  });

  it('follows the release workflow targets', () => {
    const workflow = fs.readFileSync(
      path.join(import.meta.dirname, '../../.github/workflows/release.yml'),
      'utf8',
    );
    const targets = [...workflow.matchAll(/^\s+target: (\S+)/gm)].map((match) => match[1]);
    expect(targets).toEqual(RELEASE_TARGETS);
  });
});
