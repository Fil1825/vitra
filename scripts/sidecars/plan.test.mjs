import { describe, expect, it } from 'vitest';

import {
  SIDECARS,
  SUPPORTED_TARGETS,
  YT_DLP_MACOS,
  bundledEntries,
  exeName,
  hostTarget,
  parseSha256,
  pickBtbnAsset,
  planFor,
} from './plan.mjs';

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
