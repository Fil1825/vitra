import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import sourceOffers from './upstream-source-offers.json' with { type: 'json' };
import {
  assertBtbnPins,
  assertSha256,
  pinsFromBtbnArchive,
  sha256,
  stageSourceOffers,
} from './stage-source-offers.mjs';

function fixtureArchive(rootName, files) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-offer-'));
  const root = path.join(dir, rootName);
  for (const [name, text] of Object.entries(files)) {
    const file = path.join(root, name);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, text);
  }
  const archive = path.join(dir, 'fixture.tar.gz');
  const packed = spawnSync('tar', ['-czf', archive, '-C', dir, rootName]);
  if (packed.status !== 0) throw new Error(packed.stderr.toString());
  return archive;
}

const X264 = {
  script: '50-x264.sh',
  repo: 'https://code.videolan.org/videolan/x264.git',
  commit: '0480cb05fa188d37ae87e8f4fd8f1aea3711f7ee',
};

describe('BtbN pin archive', () => {
  it('reads SCRIPT_COMMIT pins and rejects a drifted commit', () => {
    const archive = fixtureArchive('FFmpeg-Builds-fixture', {
      'scripts.d/50-x264.sh': `SCRIPT_REPO="${X264.repo}"\nSCRIPT_COMMIT="${X264.commit}"\n`,
      'scripts.d/15-base.sh': 'SCRIPT_SKIP="1"\n',
      'addins/9.0.sh': 'GIT_BRANCH="release/9.0"\n',
    });
    expect(pinsFromBtbnArchive(archive)).toEqual([X264]);
    expect(() =>
      assertBtbnPins(archive, [{ ...X264, commit: 'a'.repeat(40) }], {
        branchFile: 'addins/9.0.sh',
        branch: 'release/9.0',
      }),
    ).toThrow(/pin mismatch/);
    expect(() =>
      assertBtbnPins(archive, [X264], {
        branchFile: 'addins/9.0.sh',
        branch: 'master',
      }),
    ).toThrow(/GIT_BRANCH/);
  });
});

describe('upstream source offers', () => {
  const offers = sourceOffers.offers;
  const btbn = offers.find((offer) => offer.id === 'btbn-ffmpeg-n9.0.2-48-gd4070a14ce');
  const ytdlp = offers.find((offer) => offer.id === 'yt-dlp-2026.10.26');

  it('keeps the verified BtbN and yt-dlp archives complete', () => {
    expect(ytdlp.complete).toBe(true);
    expect(ytdlp.archives[0].sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(ytdlp.archives[0].url).toBe(
      'https://github.com/yt-dlp/yt-dlp/archive/refs/tags/2026.10.26.tar.gz',
    );
    expect(btbn.ffmpegCommit.startsWith('d4070a14ce')).toBe(true);
    expect(btbn.dependencies).toHaveLength(127);
    expect(btbn.dependencies.find((pin) => pin.script === '50-x264.sh')).toMatchObject(X264);
    expect(btbn.dependencies.find((pin) => pin.script === '50-libmp3lame.sh').svn).toBe('7113');
    for (const pin of btbn.dependencies) {
      expect(pin.commit ?? pin.svn).toBeTruthy();
    }
  });

  it('leaves Martin Riedl and official mpv incomplete', () => {
    expect(offers.find((offer) => offer.id === 'martin-riedl-macos-ffmpeg').complete).toBe(false);
    expect(offers.find((offer) => offer.id === 'mpv-official-v0.41.0').complete).toBe(false);
  });

  it('refuses to stage an archive whose bytes do not match the pin', async () => {
    const bytes = Buffer.from('not-the-archive');
    expect(() => assertSha256(bytes, sha256(Buffer.from('other')), 'fixture')).toThrow(
      /checksum mismatch/,
    );
    const offer = {
      id: 'fixture',
      component: 'yt-dlp',
      version: '0',
      license: 'Unlicense',
      complete: true,
      note: 'fixture',
      archives: [
        { name: 'fixture.bin', url: 'https://example.test/fixture.bin', sha256: sha256(bytes) },
      ],
    };
    const out = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-stage-'));
    await expect(
      stageSourceOffers(out, {
        offers: [{ ...offer, archives: [{ ...offer.archives[0], sha256: 'a'.repeat(64) }] }],
        fetch: async () => ({ ok: true, arrayBuffer: async () => bytes }),
      }),
    ).rejects.toThrow(/checksum mismatch/);
    await stageSourceOffers(out, {
      offers: [offer, offers.find((item) => item.id === 'martin-riedl-macos-ffmpeg')],
      fetch: async () => ({ ok: true, arrayBuffer: async () => bytes }),
    });
    expect(fs.existsSync(path.join(out, 'fixture.bin'))).toBe(true);
    expect(fs.existsSync(path.join(out, 'SOURCE-OFFER.txt'))).toBe(true);
    expect(fs.readFileSync(path.join(out, 'SOURCE-OFFER.txt'), 'utf8')).toContain(
      '[not complete] ffmpeg',
    );
  });
});
