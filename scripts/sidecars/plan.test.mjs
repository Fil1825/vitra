import { describe, expect, it } from 'vitest';

import {
  BTBN_PINS,
  DENO_VERSION_PIN,
  MARTIN_PINS,
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
  siblingChecksumUrl,
  sourceOffer,
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

  it('builds mpv from pinned sources on macOS and Windows', () => {
    const arm = planFor('aarch64-apple-darwin').mpv;
    expect(arm.type).toBe('source-build');
    expect(arm.script).toBe('scripts/release/build-intel-mpv.sh');
    expect(arm.sources).toBe('scripts/release/macos-arm64-mpv-sources.json');
    expect(arm.minimumMacos).toBe('14.0');
    expect(arm.downloads).toBeUndefined();
    const intel = planFor('x86_64-apple-darwin').mpv;
    expect(intel.type).toBe('source-build');
    expect(intel.script).toBe('scripts/release/build-intel-mpv.sh');
    expect(intel.minimumMacos).toBe('12.0');
    const windows = planFor('x86_64-pc-windows-msvc').mpv;
    expect(windows).toMatchObject({
      type: 'source-build',
      script: 'scripts/release/build-windows-mpv.sh',
      sources: 'scripts/release/windows-mpv-sources.json',
      executables: ['mpv/mpv.exe'],
    });
  });

  it('builds Intel mpv from pinned sources instead of a third-party binary', () => {
    const mpv = planFor('x86_64-apple-darwin').mpv;
    expect(mpv.type).toBe('source-build');
    expect(mpv.script).toBe('scripts/release/build-intel-mpv.sh');
    expect(mpv.minimumMacos).toBe('12.0');
    expect(mpv.downloads).toBeUndefined();
  });

  it('keeps the mpv app bundle intact on macOS', () => {
    const mpv = planFor('aarch64-apple-darwin').mpv;
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

  it('builds FFmpeg from pinned sources instead of BtbN or Martin Riedl', () => {
    const win = planFor('x86_64-pc-windows-msvc').ffmpeg;
    expect(win).toMatchObject({
      type: 'source-build',
      script: 'scripts/release/build-windows-mpv.sh',
      executables: ['ffmpeg.exe', 'ffprobe.exe'],
    });
    const linux = planFor('x86_64-unknown-linux-gnu').ffmpeg;
    expect(linux).toMatchObject({
      type: 'source-build',
      script: 'scripts/release/build-gpl-ffmpeg.sh',
      sources: 'scripts/release/gpl-ffmpeg-sources.json',
    });
    expect(planFor('aarch64-apple-darwin').ffmpeg.sources).toBe(
      'scripts/release/macos-arm64-mpv-sources.json',
    );
    expect(planFor('x86_64-apple-darwin').ffmpeg.sources).toBe(
      'scripts/release/intel-mpv-sources.json',
    );
    expect(sourceOffer('btbn-ffmpeg-n9.0.2-48-gd4070a14ce').complete).toBe(false);
    expect(sourceOffer('martin-riedl-macos-ffmpeg').complete).toBe(false);
    for (const pin of Object.values(BTBN_PINS)) {
      expect(pin.asset).toContain(
        sourceOffer('btbn-ffmpeg-n9.0.2-48-gd4070a14ce').ffmpegCommit.slice(0, 10),
      );
    }
    expect(MARTIN_PINS.arm64.id).toBe('1789931890_9.0.2');
  });

  it('looks up a sibling checksum on the resolved file, not the redirect', () => {
    const file = 'https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip';
    expect(siblingChecksumUrl(file, '.sha256')).toBe(`${file}.sha256`);
    expect(() =>
      siblingChecksumUrl(
        'https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip',
        '.sha256',
      ),
    ).toThrow(/redirect/);
  });

  it('lists GitHub REST release metadata for pinned GitHub-hosted binaries', () => {
    const yt = planFor('x86_64-unknown-linux-gnu')['yt-dlp'].downloads[0];
    expect(yt.githubRelease?.owner).toBe('yt-dlp');
    const ytSource = sourceOffer('yt-dlp-2026.10.26');
    expect(ytSource.version).toBe(YT_DLP_VERSION);
    expect(yt.sourceOfferId).toBe(ytSource.id);
    expect(yt.sourceClosure).toBe('complete');
    expect(yt.sourceUrls).toEqual(ytSource.archives.map((archive) => archive.url));
    expect(yt.license).toBe('Unlicense');
    const deno = planFor('x86_64-pc-windows-msvc').deno.downloads[0];
    expect(deno.githubRelease?.repo).toBe('deno');
    expect(planFor('x86_64-pc-windows-msvc').mpv.type).toBe('source-build');
    expect(planFor('x86_64-pc-windows-msvc').mpv.downloads).toBeUndefined();
  });

  it('does not expect OpenPGP signatures from sidecars that do not publish them', () => {
    const linux = planFor('x86_64-unknown-linux-gnu');
    expect(linux.deno.downloads[0].checksum.signature).toBeUndefined();
    expect(linux.ffmpeg.downloads).toBeUndefined();
    const mac = planFor('aarch64-apple-darwin');
    expect(mac.mpv.downloads).toBeUndefined();
    expect(mac.ffmpeg.downloads).toBeUndefined();
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
