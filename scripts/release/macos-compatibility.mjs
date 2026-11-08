import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

import { MPV_INTEL_STOLENDATA, MPV_VERSION } from '../sidecars/plan.mjs';

/** Vitra Info.plist minimum macOS (major.minor), per Rust target. */
export const BUNDLE_MINIMUM_MACOS = {
  'aarch64-apple-darwin': '14.0', // Sonoma — official macos-14-arm mpv
  'x86_64-apple-darwin': '12.0', // Monterey — stolendata Intel mpv 0.39 (Mach-O ≥ 11.0)
};

/** Mach-O floor of the bundled mpv package (may be below bundle minimum). */
export const MPV_MACHO_MIN_MACOS = {
  'aarch64-apple-darwin': 14,
  'x86_64-apple-darwin': 11,
};

/** Human-readable mpv source label for release notes / SBOM. */
export const BUNDLED_MPV = {
  'aarch64-apple-darwin': { version: MPV_VERSION, label: 'macos-14-arm', source: 'mpv-player' },
  'x86_64-apple-darwin': {
    version: `v${MPV_INTEL_STOLENDATA.version}`,
    label: 'stolendata-intel-0.39.0',
    source: MPV_INTEL_STOLENDATA.provenance,
    url: MPV_INTEL_STOLENDATA.homepage,
  },
};

const ROOT = path.resolve(import.meta.dirname, '../..');
const MACH_SCRIPT = path.join(import.meta.dirname, 'mach-minos.py');

export function readTauriConfigs() {
  const base = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri/tauri.conf.json'), 'utf8'));
  const intelPath = path.join(ROOT, 'src-tauri/tauri.intel.conf.json');
  const intel = fs.existsSync(intelPath) ? JSON.parse(fs.readFileSync(intelPath, 'utf8')) : null;
  return {
    armMinimum: base.bundle?.macOS?.minimumSystemVersion,
    intelMinimum: intel?.bundle?.macOS?.minimumSystemVersion ?? null,
  };
}

export function mpvAssetFor(target) {
  const spec = BUNDLED_MPV[target];
  if (!spec) {
    return null;
  }
  if (target === 'x86_64-apple-darwin') {
    return `mpv-${MPV_INTEL_STOLENDATA.version}.tar.gz`;
  }
  return `mpv-${spec.version}-${spec.label}.zip`;
}

/**
 * Every Mach-O in `root` must not require a newer macOS than `max` (fail-closed).
 */
export function verifyMachMinOs(root, max) {
  const result = spawnSync('python3', [MACH_SCRIPT, root, '--max', max], {
    encoding: 'utf8',
  });
  return {
    ok: result.status === 0,
    output: `${result.stdout ?? ''}${result.stderr ?? ''}`.trim(),
  };
}

const DEPS_SCRIPT = path.join(import.meta.dirname, 'mach-deps.py');

/** Parses LC_LOAD_DYLIB/LC_RPATH; fails on Homebrew paths or missing bundle deps. */
export function verifyMpvMachDeps(mpvAppDir) {
  const result = spawnSync('python3', [DEPS_SCRIPT, mpvAppDir, '--warn-stale-rpath'], {
    encoding: 'utf8',
  });
  return {
    ok: result.status === 0,
    output: `${result.stdout ?? ''}${result.stderr ?? ''}`.trim(),
  };
}
