import { describe, expect, it } from 'vitest';

import {
  BTBN_PINS,
  DENO_VERSION_PIN,
  SIDECARS,
  SUPPORTED_TARGETS,
  YT_DLP_MACOS,
  YT_DLP_VERSION,
  bundledEntries,
  exeName,
  hostTarget,
  parseSha256,
  pickBtbnAsset,
  planFor,
} from './plan.mjs';
import { YT_DLP_KEY_FINGERPRINT, YT_DLP_RELEASE_KEY } from './ytdlp-release.mjs';

const HASH_A = 'a'.repeat(64);
const HASH_B = 'b'.repeat(64);
const HASH_C = 'c'.repeat(64);

describe('hostTarget', () => {
  it('maps Node platforms to Rust target triples', () => {
    expect(hostTarget('darwin', 'arm64')).toBe('aarch64-apple-darwin');
    expect(hostTarget('darwin', 'x64')).toBe('x86_64-apple-darwin');
    expect(hostTarget('win32', 'x64')).toBe('x86_64-pc-windows-msvc');
    expect(hostTarget('linux', 'x64')).toBe('x86_64-unknown-linux-gnu');
  });

  it('rejects unsupported platforms', () => {
    expect(() => hostTarget('freebsd', 'x64')).toThrow(/Unsupported/);
  });
});

describe('planFor', () => {
  it.each(SUPPORTED_TARGETS)('covers every sidecar for %s', (target) => {
    const plan = planFor(target);
    expect(Object.keys(plan).sort()).toEqual([...SIDECARS].sort());
  });

  it('pins mpv builds with a checksum', () => {
    for (const target of [
      'aarch64-apple-darwin',
      'x86_64-apple-darwin',
      'x86_64-pc-windows-msvc',
    ]) {
      const [download] = planFor(target).mpv.downloads;
      expect(download.checksum.type).toBe('pinned');
      expect(download.checksum.sha256).toMatch(/^[0-9a-f]{64}$/);
      expect(download.urls[0]).toContain(download.name);
    }
  });

  it('uses stolendata Intel mpv recommended on mpv.io', () => {
    const [download] = planFor('x86_64-apple-darwin').mpv.downloads;
    expect(download.provenance).toBe('stolendata-djinn-mpv-osx');
    expect(download.archive).toBe('tar.gz');
    expect(download.checksum.sha256).toBe(
      '35ec81ad86a97b24956a8d0f4fa1ba2690b44ae7741c920e923620bcd7bd402a',
    );
  });

  it('keeps the mpv app bundle intact on macOS', () => {
    const mpv = planFor('aarch64-apple-darwin').mpv;
    expect(mpv.downloads[0].files).toEqual([{ find: 'mpv.app', to: 'mpv/mpv.app' }]);
    expect(mpv.executables).toEqual(['mpv/mpv.app/Contents/MacOS/mpv']);
  });

  it('fetches the folder build of yt-dlp on macOS', () => {
    const ytDlp = planFor('aarch64-apple-darwin')['yt-dlp'];
    const [download] = ytDlp.downloads;
    expect(download.name).toBe('yt-dlp_macos.zip');
    expect(download.archive).toBe('zip');
    expect(download.checksum).toMatchObject({ type: 'sums-file', entry: 'yt-dlp_macos.zip' });
    expect(download.files).toEqual([{ root: true, to: 'yt-dlp' }]);
    expect(ytDlp.executables).toEqual([YT_DLP_MACOS]);
    expect(YT_DLP_MACOS).toBe('yt-dlp/yt-dlp_macos');
  });

  it('requires an OpenPGP signature over the yt-dlp checksum file', () => {
    for (const target of SUPPORTED_TARGETS) {
      const [download] = planFor(target)['yt-dlp'].downloads;
      expect(download.checksum.url).toMatch(/\/SHA2-256SUMS$/);
      expect(download.checksum.vendored).toBe('scripts/sidecars/fixtures/ytdlp-SHA2-256SUMS');
      expect(download.checksum.signature).toEqual({
        url: download.checksum.url + '.sig',
        vendored: 'scripts/sidecars/fixtures/ytdlp-SHA2-256SUMS.sig',
        key: YT_DLP_RELEASE_KEY,
        fingerprint: YT_DLP_KEY_FINGERPRINT,
      });
    }
  });

  it('pins sidecar release tags instead of GitHub /latest/download', () => {
    for (const target of SUPPORTED_TARGETS) {
      const yt = planFor(target)['yt-dlp'].downloads[0];
      expect(yt.urls[0]).toContain(`/releases/download/${YT_DLP_VERSION}/`);
      expect(yt.urls[0]).not.toContain('/latest/');
      const deno = planFor(target).deno.downloads[0];
      expect(deno.urls[0]).toContain(`/releases/download/${DENO_VERSION_PIN}/`);
      expect(deno.checksum.type).toBe('pinned');
    }
  });

  it('pins BtbN FFmpeg assets on Windows and Linux', () => {
    const win = planFor('x86_64-pc-windows-msvc').ffmpeg.downloads[0];
    expect(win).toMatchObject({
      name: BTBN_PINS.win64.asset,
      checksum: { type: 'pinned', sha256: BTBN_PINS.win64.sha256 },
    });
    const linux = planFor('x86_64-unknown-linux-gnu').ffmpeg.downloads[0];
    expect(linux.checksum.sha256).toBe(BTBN_PINS.linux64.sha256);
  });

  it('does not expect OpenPGP signatures from sidecars that do not publish them', () => {
    const linux = planFor('x86_64-unknown-linux-gnu');
    expect(linux.deno.downloads[0].checksum.signature).toBeUndefined();
    expect(linux.ffmpeg.downloads[0].signature).toBeUndefined();
    const mac = planFor('aarch64-apple-darwin');
    expect(mac.mpv.downloads[0].checksum.signature).toBeUndefined();
    expect(mac.ffmpeg.downloads[0].checksum.signature).toBeUndefined();
    expect(mac.deno.downloads[0].checksum.signature).toBeUndefined();
  });

  it('fetches single-file yt-dlp builds elsewhere', () => {
    const windows = planFor('x86_64-pc-windows-msvc')['yt-dlp'];
    expect(windows.downloads[0]).toMatchObject({ name: 'yt-dlp.exe', archive: null });
    expect(windows.executables).toEqual(['yt-dlp.exe']);
    const linux = planFor('x86_64-unknown-linux-gnu')['yt-dlp'];
    expect(linux.downloads[0].name).toBe('yt-dlp_linux');
    expect(linux.executables).toEqual(['yt-dlp']);
  });

  it('uses the system mpv on Linux', () => {
    expect(planFor('x86_64-unknown-linux-gnu').mpv.type).toBe('system');
  });

  it('adds .exe on Windows only', () => {
    expect(exeName('x86_64-pc-windows-msvc', 'deno')).toBe('deno.exe');
    expect(exeName('aarch64-apple-darwin', 'deno')).toBe('deno');
    expect(planFor('x86_64-pc-windows-msvc').ffmpeg.executables).toEqual([
      'ffmpeg.exe',
      'ffprobe.exe',
    ]);
  });

  it('rejects unknown targets', () => {
    expect(() => planFor('riscv64gc-unknown-linux-gnu')).toThrow(/Unsupported target/);
  });
});

describe('bundledEntries', () => {
  it('ships the programs only', () => {
    expect(
      bundledEntries(['yt-dlp', 'manifest.json', '.work-abc', 'mpv', 'ffprobe', 'ffmpeg']),
    ).toEqual(['ffmpeg', 'ffprobe', 'mpv', 'yt-dlp']);
  });
});

describe('pickBtbnAsset', () => {
  const sums = [
    `${HASH_A}  ffmpeg-master-latest-win64-gpl.zip`,
    `${HASH_B}  ffmpeg-n8.1-latest-win64-gpl-8.1.zip`,
    `${HASH_C}  ffmpeg-n9.0-latest-win64-gpl-9.0.zip`,
    `${HASH_A}  ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip`,
    `${HASH_A}  ffmpeg-n9.0-latest-win64-lgpl-9.0.zip`,
    `${HASH_B}  ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz`,
  ].join('\n');

  it('picks the newest stable static GPL build', () => {
    expect(pickBtbnAsset(sums, 'win64', 'zip')).toEqual({
      asset: 'ffmpeg-n9.0-latest-win64-gpl-9.0.zip',
      sha256: HASH_C,
    });
    expect(pickBtbnAsset(sums, 'linux64', 'tar.xz')?.asset).toBe(
      'ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz',
    );
  });

  it('compares versions numerically', () => {
    const text = `${HASH_A}  ffmpeg-n9.0-latest-win64-gpl-9.0.zip\n${HASH_B}  ffmpeg-n10.0-latest-win64-gpl-10.0.zip`;
    expect(pickBtbnAsset(text, 'win64', 'zip')?.sha256).toBe(HASH_B);
  });

  it('falls back to the master build', () => {
    const text = `${HASH_A}  ffmpeg-master-latest-linuxarm64-gpl.tar.xz`;
    expect(pickBtbnAsset(text, 'linuxarm64', 'tar.xz')).toEqual({
      asset: 'ffmpeg-master-latest-linuxarm64-gpl.tar.xz',
      sha256: HASH_A,
    });
  });

  it('returns null when nothing matches', () => {
    expect(pickBtbnAsset(sums, 'winarm64', 'zip')).toBeNull();
  });
});

describe('parseSha256', () => {
  it('looks up an entry in a sums file', () => {
    const text = `${HASH_A}  yt-dlp\n${HASH_B}  yt-dlp_macos\n${HASH_C} *yt-dlp.exe\n`;
    expect(parseSha256(text, 'yt-dlp_macos')).toBe(HASH_B);
    expect(parseSha256(text, 'yt-dlp.exe')).toBe(HASH_C);
    expect(parseSha256(text, 'yt-dlp_linux')).toBeNull();
  });

  it('reads sha256sum output', () => {
    expect(parseSha256(`${HASH_A}  deno-aarch64-apple-darwin.zip\n`)).toBe(HASH_A);
  });

  it('reads PowerShell Get-FileHash output', () => {
    const text = `\nAlgorithm : SHA256\nHash      : ${HASH_B.toUpperCase()}\nPath      : C:\\deno.zip\n`;
    expect(parseSha256(text)).toBe(HASH_B);
  });

  it('returns null without a hash', () => {
    expect(parseSha256('404: Not Found')).toBeNull();
  });
});
