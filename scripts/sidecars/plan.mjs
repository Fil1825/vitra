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

import { sourceClosureFor } from '../release/source-kit-gate.mjs';
import sourceOffers from '../release/upstream-source-offers.json' with { type: 'json' };
import { YT_DLP_KEY_FINGERPRINT, YT_DLP_RELEASE_KEY, YT_DLP_VERSION } from './ytdlp-release.mjs';

export { YT_DLP_VERSION };

export const SIDECARS = ['mpv', 'yt-dlp', 'ffmpeg', 'deno'];

/** mpv tag built by the pinned source recipes (not an official binary asset). */
export const MPV_VERSION = 'v0.41.0';

/** Intel macOS mpv is built from scripts/release/intel-mpv-sources.json (not a third-party binary). */
export const MPV_INTEL_SOURCE_BUILD = {
  script: 'scripts/release/build-intel-mpv.sh',
  sources: 'scripts/release/intel-mpv-sources.json',
  minimumMacos: '12.0',
};

const MPV_ARM_SOURCE_BUILD = {
  script: 'scripts/release/build-intel-mpv.sh',
  sources: 'scripts/release/macos-arm64-mpv-sources.json',
  minimumMacos: '14.0',
};

const MPV_WINDOWS_SOURCE_BUILD = {
  script: 'scripts/release/build-windows-mpv.sh',
  sources: 'scripts/release/windows-mpv-sources.json',
};

const GPL_FFMPEG_SOURCE_BUILD = {
  script: 'scripts/release/build-gpl-ffmpeg.sh',
  sources: 'scripts/release/gpl-ffmpeg-sources.json',
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

/**
 * Martin Riedl macOS 9.0.2 zips. The current plan does not download them.
 * The digests stay so a later check can still recognise those bytes.
 */
export const MARTIN_PINS = {
  arm64: {
    id: '1789931890_9.0.2',
    ffmpeg: 'c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924',
    ffprobe: 'fcbe839537485eaee7a7a8bc5cbc0f90d53617e80943e8a5b2e31cb851197ea6',
  },
  amd64: {
    id: '1789931006_9.0.2',
    ffmpeg: '7c6b4125b191cbf773832dc51f424cf2b6bb7da43007d1e066f95909e47cacd4',
    ffprobe: '2322438ed2f6319a691291b247d09c69dcaa3a982460d1f269a7e1af335cfdfd',
  },
};

/**
 * Checksum published beside the file that was downloaded. `redirect/latest`
 * has no sibling `.sha256`; the versioned URL does.
 */
export function siblingChecksumUrl(resolvedFileUrl, suffix) {
  if (typeof resolvedFileUrl !== 'string' || typeof suffix !== 'string' || !suffix) {
    throw new Error('sibling checksum requires the resolved file URL');
  }
  let url;
  try {
    url = new URL(resolvedFileUrl);
  } catch {
    throw new Error(`sibling checksum URL is not absolute: ${resolvedFileUrl}`);
  }
  if (url.pathname.includes('/redirect/')) {
    throw new Error(
      `sibling checksum must use the resolved file, not a redirect URL: ${resolvedFileUrl}`,
    );
  }
  return `${resolvedFileUrl}${suffix}`;
}

/**
 * Dated BtbN autobuild release tag (not the rolling `latest` slot). Assets are fixed for this
 * tag; hashes from that release's checksums.sha256 at bump time.
 */
export const BTBN_RELEASE_TAG = 'autobuild-2026-11-07-12-47';

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

/** Pinned BtbN GPL static builds on {@link BTBN_RELEASE_TAG}. */
export const BTBN_PINS = {
  win64: {
    asset: 'ffmpeg-n9.0.2-48-gd4070a14ce-win64-gpl-9.0.zip',
    sha256: 'b33f1e3383f97c7acc8502e475df2faabcaee7d9605facda9b213cfbb39e0300',
    assetId: 720692322,
  },
  linux64: {
    asset: 'ffmpeg-n9.0.2-48-gd4070a14ce-linux64-gpl-9.0.tar.xz',
    sha256: '5ce093566d5fe635a7bfc01b453d780ddee48b7e8c08021b45b877093d3efeec',
    assetId: 720693414,
  },
  linuxarm64: {
    asset: 'ffmpeg-n9.0.2-48-gd4070a14ce-linuxarm64-gpl-9.0.tar.xz',
    sha256: '21f10b6a85bc3ad6a53c2f54287aec9cbc9b186d9074ac14e4efd07853ca6d83',
    assetId: 720693549,
  },
};

function ytDlpGithubRelease(asset) {
  return { owner: 'yt-dlp', repo: 'yt-dlp', tag: YT_DLP_VERSION, asset };
}

/** Corresponding-source offer, or null when the id is not in the manifest. */
export function sourceOffer(id) {
  return sourceOffers.offers.find((offer) => offer.id === id) ?? null;
}

// Attaches the offer id and the archives that actually have URLs.
// `sourceClosure` is derived from pins and license paths, never from `offer.complete`.
function sourceLink(id, platform) {
  const offer = sourceOffer(id);
  if (!offer) {
    throw new Error(`unknown source offer ${id}`);
  }
  const link = {
    sourceOfferId: id,
    license: offer.license,
    sourceClosure: sourceClosureFor(offer, platform),
  };
  if (offer.archives?.length) {
    link.sourceUrls = offer.archives.map((archive) => archive.url);
  }
  return link;
}

function denoGithubRelease(asset) {
  return { owner: 'denoland', repo: 'deno', tag: DENO_VERSION, asset };
}

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
    ffmpeg: ffmpegPlan(target, info, exe),
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
        githubRelease: denoGithubRelease(name),
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
    githubRelease: ytDlpGithubRelease(info.ytDlp),
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
    ...sourceLink('yt-dlp-2026.10.26', { os: info.os }),
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

function sourceBuild(spec, executables) {
  return {
    type: 'source-build',
    script: spec.script,
    sources: spec.sources,
    ...(spec.minimumMacos ? { minimumMacos: spec.minimumMacos } : {}),
    executables,
    versionArgs: executables[0].includes('mpv') ? ['--version'] : ['-version'],
  };
}

function macosMpvSpec(target) {
  return target === 'aarch64-apple-darwin' ? MPV_ARM_SOURCE_BUILD : MPV_INTEL_SOURCE_BUILD;
}

function mpvPlan(target, info) {
  if (info.os === 'linux') {
    return {
      type: 'system',
      binary: 'mpv',
      hint: 'mpv has no official Linux binaries. Install it with your package manager, e.g. `sudo apt install mpv`.',
    };
  }
  if (info.os === 'macos') {
    return sourceBuild(macosMpvSpec(target), ['mpv/mpv.app/Contents/MacOS/mpv']);
  }
  return sourceBuild(MPV_WINDOWS_SOURCE_BUILD, ['mpv/mpv.exe']);
}

function ffmpegPlan(target, info, exe) {
  if (info.os === 'macos') {
    return sourceBuild(macosMpvSpec(target), ['ffmpeg', 'ffprobe']);
  }
  if (info.os === 'windows') {
    return sourceBuild(MPV_WINDOWS_SOURCE_BUILD, [exe('ffmpeg'), exe('ffprobe')]);
  }
  return sourceBuild(GPL_FFMPEG_SOURCE_BUILD, [exe('ffmpeg'), exe('ffprobe')]);
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
