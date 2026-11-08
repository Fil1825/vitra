import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

import { MPV_VERSION } from '../sidecars/plan.mjs';

/** Vitra Info.plist minimum macOS (major.minor), per Rust target. */
export const BUNDLE_MINIMUM_MACOS = {
  'aarch64-apple-darwin': '14.0', // Sonoma — matches official macos-14-arm mpv
  'x86_64-apple-darwin': '15.0', // Sequoia — matches mpv-v0.41.0-macos-15-intel main binary
};

/** Official pinned mpv asset id (see scripts/sidecars/plan.mjs). */
export const BUNDLED_MPV = {
  'aarch64-apple-darwin': 'macos-14-arm',
  'x86_64-apple-darwin': 'macos-15-intel',
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
  const flavor = BUNDLED_MPV[target];
  return flavor ? `mpv-${MPV_VERSION}-${flavor}.zip` : null;
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
