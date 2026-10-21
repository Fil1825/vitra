// Pure description of which helper binaries to download for each target.
// Kept free of I/O so it can be unit-tested; scripts/fetch-sidecars.mjs does the work.
//
// Layout produced in src-tauri/sidecars/<target triple>/ (must match src-tauri/src/sidecar.rs):
//   mpv/                 mpv with its shared libraries (mpv.app on macOS, mpv.exe + DLLs on Windows)
//   yt-dlp[.exe]
//   ffmpeg[.exe], ffprobe[.exe]
//   deno[.exe]           JavaScript runtime yt-dlp needs for YouTube
//   manifest.json        what was fetched, from where, with which checksum

export const SIDECARS = ['mpv', 'yt-dlp', 'ffmpeg', 'deno'];

/** Official mpv builds, pinned with checksums computed when the version was bumped. */
export const MPV_VERSION = 'v0.41.0';
const MPV_BUILDS = {
  'aarch64-apple-darwin': {
    asset: `mpv-${MPV_VERSION}-macos-14-arm.zip`,
    sha256: '5c96f9b21355fc0a11d2e2161ad65f33031070e9fb3f6bd9865fb459b94587e6',
  },
  'x86_64-apple-darwin': {
    asset: `mpv-${MPV_VERSION}-macos-15-intel.zip`,
    sha256: '41003617ab4f7784394b5ddea7ce51b3e0838e8cfc8166ad1a378b2eda3b583c',
  },
  'x86_64-pc-windows-msvc': {
    asset: `mpv-${MPV_VERSION}-x86_64-pc-windows-msvc.zip`,
    sha256: '4e197f729f5071c6772f35fffd96e0f36e3e8a044bd9479b136bb09b7c6a80ff',
  },
};

const TARGETS = {
  'aarch64-apple-darwin': { os: 'macos', ytDlp: 'yt-dlp_macos', ffmpeg: 'arm64' },
  'x86_64-apple-darwin': { os: 'macos', ytDlp: 'yt-dlp_macos', ffmpeg: 'amd64' },
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
const YT_DLP_RELEASE = `${GITHUB}/yt-dlp/yt-dlp/releases/latest/download`;
const DENO_RELEASE = `${GITHUB}/denoland/deno/releases/latest/download`;
const BTBN_RELEASE = `${GITHUB}/BtbN/FFmpeg-Builds/releases/download/latest`;
const MARTIN_RIEDL = 'https://ffmpeg.martin-riedl.de/redirect/latest/macos';

/**
 * Returns what to fetch for every sidecar on `target`.
 *
 * Each sidecar is either `{ type: 'system' }` (must come from the OS package manager) or
 * `{ type: 'download', downloads, executables, versionArgs }`. A download lists candidate
 * `urls` (tried in order), how to verify its `checksum`, its `archive` format and the
 * `files` to move into the sidecar directory.
 */
export function planFor(target) {
  const info = TARGETS[target];
  if (!info) {
    throw new Error(`Unsupported target ${target}. Supported: ${SUPPORTED_TARGETS.join(', ')}`);
  }
  const exe = (name) => exeName(target, name);

  return {
    mpv: mpvPlan(target, info),
    'yt-dlp': {
      type: 'download',
      downloads: [
        {
          name: info.ytDlp,
          urls: [`${YT_DLP_RELEASE}/${info.ytDlp}`],
          checksum: { type: 'sums-file', url: `${YT_DLP_RELEASE}/SHA2-256SUMS`, entry: info.ytDlp },
          archive: null,
          files: [{ to: exe('yt-dlp') }],
        },
      ],
      executables: [exe('yt-dlp')],
      versionArgs: ['--version'],
    },
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
    deno: {
      type: 'download',
      downloads: [
        {
          name: `deno-${target}.zip`,
          urls: [`${DENO_RELEASE}/deno-${target}.zip`],
          checksum: { type: 'sidecar-file', url: `${DENO_RELEASE}/deno-${target}.zip.sha256sum` },
          archive: 'zip',
          files: [{ find: exe('deno'), to: exe('deno') }],
        },
      ],
      executables: [exe('deno')],
      versionArgs: ['--version'],
    },
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
  const build = MPV_BUILDS[target];
  const download = {
    name: build.asset,
    urls: [`${GITHUB}/mpv-player/mpv/releases/download/${MPV_VERSION}/${build.asset}`],
    checksum: { type: 'pinned', sha256: build.sha256 },
    archive: 'zip',
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

/** Static Windows/Linux builds by BtbN; the exact asset is picked from the release checksums. */
function btbnDownload(info, exe) {
  return {
    type: 'btbn',
    platform: info.ffmpeg,
    ext: info.os === 'windows' ? 'zip' : 'tar.xz',
    checksumsUrl: `${BTBN_RELEASE}/checksums.sha256`,
    releaseUrl: BTBN_RELEASE,
    archive: info.os === 'windows' ? 'zip' : 'tar',
    files: [exe('ffmpeg'), exe('ffprobe')].map((f) => ({ find: f, to: f })),
  };
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
