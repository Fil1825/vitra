// Pure description of which helper binaries to download for each target.
// Kept free of I/O so it can be unit-tested; scripts/fetch-sidecars.mjs does the work.
//
// Layout produced in src-tauri/sidecars/<target triple>/ (must match src-tauri/src/sidecar.rs):
//   mpv/                 mpv with its shared libraries (mpv.app on macOS, mpv.exe + DLLs on Windows)
//   yt-dlp[.exe]         a single file on Windows and Linux;
//   yt-dlp/yt-dlp_macos  on macOS a folder with the program and its `_internal/` libraries
//   ffmpeg[.exe], ffprobe[.exe]
//   deno[.exe]           JavaScript runtime yt-dlp needs for YouTube
//   manifest.json        what was fetched, from where, with which checksum

import { YT_DLP_KEY_FINGERPRINT, YT_DLP_RELEASE_KEY, YT_DLP_VERSION } from './ytdlp-release.mjs';

export { YT_DLP_VERSION };

export const SIDECARS = ['mpv', 'yt-dlp', 'ffmpeg', 'deno'];

/** Official mpv builds (arm / Windows), pinned with checksums at bump time. */
export const MPV_VERSION = 'v0.41.0';

/**
 * Intel macOS mpv: third-party bundle from stolendata (djinn), recommended on mpv.io/installation.
 * Not an official mpv-player GitHub release; GPLv2+ — see docs/dev/MACOS-INTEL-MONTEREY.md.
 */
export const MPV_INTEL_STOLENDATA = {
  version: '0.39.0',
  url: 'https://laboratory.stolendata.net/~djinn/mpv_osx/mpv-0.39.0.tar.gz',
  sha256: '35ec81ad86a97b24956a8d0f4fa1ba2690b44ae7741c920e923620bcd7bd402a',
  provenance: 'stolendata-djinn-mpv-osx',
  homepage: 'https://laboratory.stolendata.net/~djinn/mpv_osx/',
};

const MPV_BUILDS = {
  'aarch64-apple-darwin': {
    asset: `mpv-${MPV_VERSION}-macos-14-arm.zip`,
    sha256: '5c96f9b21355fc0a11d2e2161ad65f33031070e9fb3f6bd9865fb459b94587e6',
    source: 'mpv-player-github',
  },
  'x86_64-apple-darwin': MPV_INTEL_STOLENDATA,
  'x86_64-pc-windows-msvc': {
    asset: `mpv-${MPV_VERSION}-x86_64-pc-windows-msvc.zip`,
    sha256: '4e197f729f5071c6772f35fffd96e0f36e3e8a044bd9479b136bb09b7c6a80ff',
    source: 'mpv-player-github',
  },
};

const TARGETS = {
  'aarch64-apple-darwin': { os: 'macos', ytDlp: 'yt-dlp_macos.zip', ffmpeg: 'arm64' },
  'x86_64-apple-darwin': { os: 'macos', ytDlp: 'yt-dlp_macos.zip', ffmpeg: 'amd64' },
  'x86_64-pc-windows-msvc': { os: 'windows', ytDlp: 'yt-dlp.exe', ffmpeg: 'win64' },
  'x86_64-unknown-linux-gnu': { os: 'linux', ytDlp: 'yt-dlp_linux', ffmpeg: 'linux64' },
  'aarch64-unknown-linux-gnu': { os: 'linux', ytDlp: 'yt-dlp_linux_aarch64', ffmpeg: 'linuxarm64' },
};

export const SUPPORTED_TARGETS = Object.keys(TARGETS);

const HOST_TARGETS = {
  'darwin-arm64': 'aarch64-apple-darwin',
  'darwin-x64': 'x86_64-apple-darwin',
  'win32-x64': 'x86_64-pc-windows-msvc',
  'linux-x64': 'x86_64-unknown-linux-gnu',
  'linux-arm64': 'aarch64-unknown-linux-gnu',
};

/** Maps Node's platform/arch to a Rust target triple. */
export function hostTarget(platform = process.platform, arch = process.arch) {
  const target = HOST_TARGETS[`${platform}-${arch}`];
  if (!target) {
    throw new Error(`Unsupported platform ${platform}-${arch}`);
  }
  return target;
}

export function exeName(target, name) {
  return TARGETS[target]?.os === 'windows' ? `${name}.exe` : name;
}

const GITHUB = 'https://github.com';
const YT_DLP_RELEASE = `${GITHUB}/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}`;
const DENO_VERSION = 'v2.9.8';
const DENO_RELEASE = `${GITHUB}/denoland/deno/releases/download/${DENO_VERSION}`;
const BTBN_RELEASE = `${GITHUB}/BtbN/FFmpeg-Builds/releases/download/latest`;
const MARTIN_RIEDL = 'https://ffmpeg.martin-riedl.de/redirect/latest/macos';

export const DENO_VERSION_PIN = DENO_VERSION;

/** Pinned at bump time from denoland/deno release checksum sidecars (v2.9.8). */
const DENO_SHA256 = {
  'aarch64-apple-darwin': '9f0bf660393da2eaff044cdc7444cb08c73f235849b61d224d825a194ce7c98e',
  'x86_64-apple-darwin': '4b2f5edd2639b7470f034238b321d7002d8dfb4726d8558f52c17afac370eb2c',
  'x86_64-pc-windows-msvc': 'cba84b26e0df5ae7f74fc66b86a95b866872a86292ebd84fa1e959927edd93bf',
  'x86_64-unknown-linux-gnu': 'e890975bc2549807bbcc0b584ffafda72cc7c93349dd99c80483dd784f6c4609',
  'aarch64-unknown-linux-gnu': 'd49596c20b9788cf8f5d8ddf644103d4bde56ed239753f0bdb92ba9361d51905',
};

const YT_DLP_SUMS_VENDORED = 'scripts/sidecars/fixtures/ytdlp-SHA2-256SUMS';
const YT_DLP_SIG_VENDORED = 'scripts/sidecars/fixtures/ytdlp-SHA2-256SUMS.sig';

/** Pinned BtbN GPL static builds (n9.0); hashes from upstream checksums.sha256 at bump time. */
const BTBN_FFMPEG_VERSION = '9.0';
export const BTBN_PINS = {
  win64: {
    asset: `ffmpeg-n${BTBN_FFMPEG_VERSION}-latest-win64-gpl-${BTBN_FFMPEG_VERSION}.zip`,
    sha256: 'e7c71f39ec327bb5c4f0085c9fa4ba490caa057d5742e0ed1e1b36001d8a1e81',
  },
  linux64: {
    asset: `ffmpeg-n${BTBN_FFMPEG_VERSION}-latest-linux64-gpl-${BTBN_FFMPEG_VERSION}.tar.xz`,
    sha256: '6ad96ff7dd4815a31522f1507ea81b7b9b1adf59da93921151a625b44a307185',
  },
  linuxarm64: {
    asset: `ffmpeg-n${BTBN_FFMPEG_VERSION}-latest-linuxarm64-gpl-${BTBN_FFMPEG_VERSION}.tar.xz`,
    sha256: '88f8df5fcc621e3ee6c642b8b0b914a0929a61e909704d69f9af5aaecccda7ce',
  },
};

/**
 * Returns what to fetch for every sidecar on `target`.
 *
 * Each sidecar is either `{ type: 'system' }` (must come from the OS package manager) or
 * `{ type: 'download', downloads, executables, versionArgs }`. A download lists candidate
 * `urls` (tried in order), how to verify its `checksum`, its `archive` format and the
 * `files` to move into the sidecar directory: `{ find, to }` moves one entry of the archive,
 * `{ root: true, to }` the whole unpacked archive, `{ to }` the downloaded file itself.
 * A checksum may include `signature` (`url`, `key`, `fingerprint`). Only yt-dlp sets it:
 * the checksum file is then accepted only with a valid detached OpenPGP signature.
 */
export function planFor(target) {
  const info = TARGETS[target];
  if (!info) {
    throw new Error(`Unsupported target ${target}. Supported: ${SUPPORTED_TARGETS.join(', ')}`);
  }
  const exe = (name) => exeName(target, name);

  return {
    mpv: mpvPlan(target, info),
    'yt-dlp': ytDlpPlan(info, exe),
    ffmpeg: {
      type: 'download',
      downloads:
        info.os === 'macos' ? martinRiedlDownloads(info.ffmpeg) : [btbnDownload(info, exe)],
      executables: [exe('ffmpeg'), exe('ffprobe')],
      versionArgs: ['-version'],
      ...(info.os === 'macos' && {
        hint: 'Alternatively run `brew install ffmpeg`: Vitra also finds FFmpeg in Homebrew.',
      }),
    },
    deno: denoPlan(target, exe),
  };
}

/** Where yt-dlp's program lives in the sidecar folder on macOS (see `ytDlpPlan`). */
export const YT_DLP_MACOS = 'yt-dlp/yt-dlp_macos';

function denoPlan(target, exe) {
  const name = `deno-${target}.zip`;
  const sha256 = DENO_SHA256[target];
  if (!sha256) {
    throw new Error(`no pinned deno checksum for ${target}`);
  }
  return {
    type: 'download',
    downloads: [
      {
        name,
        urls: [`${DENO_RELEASE}/${name}`],
        checksum: { type: 'pinned', sha256 },
        archive: 'zip',
        files: [{ find: exe('deno'), to: exe('deno') }],
      },
    ],
    executables: [exe('deno')],
    versionArgs: ['--version'],
  };
}

function ytDlpPlan(info, exe) {
  const download = {
    name: info.ytDlp,
    urls: [`${YT_DLP_RELEASE}/${info.ytDlp}`],
    checksum: {
      type: 'sums-file',
      url: `${YT_DLP_RELEASE}/SHA2-256SUMS`,
      vendored: YT_DLP_SUMS_VENDORED,
      entry: info.ytDlp,
      // Same release as the binary. The signature is what ties the sums to yt-dlp's key.
      signature: {
        url: `${YT_DLP_RELEASE}/SHA2-256SUMS.sig`,
        vendored: YT_DLP_SIG_VENDORED,
        key: YT_DLP_RELEASE_KEY,
        fingerprint: YT_DLP_KEY_FINGERPRINT,
      },
    },
  };
  if (info.os === 'macos') {
    // Not the single-file build: that one unpacks Python into a fresh temporary folder on
    // every run, and macOS checks each of those libraries again (~25 s per run). The folder
    // build is checked once. It can't update itself; Vitra downloads the zip again instead.
    return {
      type: 'download',
      downloads: [{ ...download, archive: 'zip', files: [{ root: true, to: 'yt-dlp' }] }],
      executables: [YT_DLP_MACOS],
      versionArgs: ['--version'],
    };
  }
  return {
    type: 'download',
    downloads: [{ ...download, archive: null, files: [{ to: exe('yt-dlp') }] }],
    executables: [exe('yt-dlp')],
    versionArgs: ['--version'],
  };
}

function mpvPlan(target, info) {
  if (info.os === 'linux') {
    return {
      type: 'system',
      binary: 'mpv',
      hint: 'mpv has no official Linux binaries. Install it with your package manager, e.g. `sudo apt install mpv`.',
    };
  }
  if (target === 'x86_64-apple-darwin') {
    const build = MPV_BUILDS[target];
    return {
      type: 'download',
      downloads: [
        {
          name: `mpv-${build.version}.tar.gz`,
          urls: [build.url],
          checksum: { type: 'pinned', sha256: build.sha256 },
          archive: 'tar.gz',
          provenance: build.provenance,
          sourceUrl: build.homepage,
          license:
            'GPL-2.0-or-later (third-party build; sources: https://github.com/mpv-player/mpv)',
          files: [{ find: 'mpv.app', to: 'mpv/mpv.app' }],
        },
      ],
      executables: ['mpv/mpv.app/Contents/MacOS/mpv'],
      versionArgs: ['--version'],
    };
  }
  const build = MPV_BUILDS[target];
  const download = {
    name: build.asset,
    urls: [`${GITHUB}/mpv-player/mpv/releases/download/${MPV_VERSION}/${build.asset}`],
    checksum: { type: 'pinned', sha256: build.sha256 },
    archive: 'zip',
    provenance: build.source,
  };
  if (info.os === 'macos') {
    return {
      type: 'download',
      // The zip wraps a tarball that preserves the app bundle's code signature.
      downloads: [
        { ...download, nested: 'mpv.tar.gz', files: [{ find: 'mpv.app', to: 'mpv/mpv.app' }] },
      ],
      executables: ['mpv/mpv.app/Contents/MacOS/mpv'],
      versionArgs: ['--version'],
    };
  }
  return {
    type: 'download',
    downloads: [
      {
        ...download,
        files: ['mpv.exe', 'mpv.com', 'vulkan-1.dll'].map((f) => ({ find: f, to: `mpv/${f}` })),
      },
    ],
    executables: ['mpv/mpv.exe'],
    versionArgs: ['--version'],
  };
}

/** Static macOS builds by Martin Riedl; the snapshot channel is a fallback if no release zip exists. */
function martinRiedlDownloads(arch) {
  return ['ffmpeg', 'ffprobe'].map((tool) => ({
    name: `${tool}.zip`,
    urls: ['release', 'snapshot'].map(
      (channel) => `${MARTIN_RIEDL}/${arch}/${channel}/${tool}.zip`,
    ),
    checksum: { type: 'sibling', suffix: '.sha256' },
    archive: 'zip',
    files: [{ find: tool, to: tool }],
  }));
}

/** Static Windows/Linux builds by BtbN; pinned so CI does not depend on live checksums.sha256. */
function btbnDownload(info, exe) {
  const pin = BTBN_PINS[info.ffmpeg];
  if (!pin) {
    throw new Error(`no pinned BtbN FFmpeg build for platform ${info.ffmpeg}`);
  }
  return {
    name: pin.asset,
    urls: [`${BTBN_RELEASE}/${pin.asset}`],
    checksum: { type: 'pinned', sha256: pin.sha256 },
    archive: info.os === 'windows' ? 'zip' : 'tar',
    files: [exe('ffmpeg'), exe('ffprobe')].map((f) => ({ find: f, to: f })),
  };
}

/**
 * What of a fetched sidecar folder goes into the app bundle: the programs, not the
 * manifest or leftovers of an interrupted fetch.
 */
export function bundledEntries(names) {
  return names.filter((name) => name !== 'manifest.json' && !name.startsWith('.')).sort();
}

/**
 * Picks the newest stable GPL build for `platform` from BtbN's checksums file,
 * falling back to the master build. Returns `{ asset, sha256 }` or null.
 */
export function pickBtbnAsset(checksumsText, platform, ext) {
  const escapedExt = ext.replace('.', '\\.');
  const stable = new RegExp(
    `^ffmpeg-n(\\d+)\\.(\\d+)-latest-${platform}-gpl-\\1\\.\\2\\.${escapedExt}$`,
  );
  const master = `ffmpeg-master-latest-${platform}-gpl.${ext}`;

  let best = null;
  let fallback = null;
  for (const { sha256, name } of parseSumsFile(checksumsText)) {
    const m = stable.exec(name);
    if (m) {
      const version = [Number(m[1]), Number(m[2])];
      if (!best || compareVersions(version, best.version) > 0) {
        best = { asset: name, sha256, version };
      }
    } else if (name === master) {
      fallback = { asset: name, sha256 };
    }
  }
  return best ? { asset: best.asset, sha256: best.sha256 } : fallback;
}

function compareVersions(a, b) {
  return a[0] - b[0] || a[1] - b[1];
}

/** Parses `sha256sum`-style lines: `<hash>  <name>` or `<hash> *<name>`. */
export function parseSumsFile(text) {
  const entries = [];
  for (const line of text.split(/\r?\n/)) {
    const m = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (m) {
      entries.push({ sha256: m[1].toLowerCase(), name: m[2] });
    }
  }
  return entries;
}

/**
 * Extracts a SHA-256 from a checksum file. With `entry`, looks it up in a multi-file
 * sums file; otherwise takes the first hash in the text (covers both `sha256sum` output
 * and PowerShell's `Get-FileHash` format).
 */
export function parseSha256(text, entry) {
  if (entry) {
    return parseSumsFile(text).find((e) => e.name === entry)?.sha256 ?? null;
  }
  const m = /\b([0-9a-fA-F]{64})\b/.exec(text);
  return m ? m[1].toLowerCase() : null;
}
