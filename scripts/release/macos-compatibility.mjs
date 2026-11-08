import fs from 'node:fs';
import path from 'node:path';

import { MPV_VERSION } from '../sidecars/plan.mjs';

/** mpv macOS builds packaged for each Vitra target (see scripts/sidecars/plan.mjs). */
export const BUNDLED_MPV = {
  'aarch64-apple-darwin': { asset: 'macos-14-arm', minimumMacOS: 14 },
  'x86_64-apple-darwin': { asset: 'macos-15-intel', minimumMacOS: 15 },
};

const ROOT = path.resolve(import.meta.dirname, '../..');

export function readTauriConfigs() {
  const base = JSON.parse(fs.readFileSync(path.join(ROOT, 'src-tauri/tauri.conf.json'), 'utf8'));
  const intelPath = path.join(ROOT, 'src-tauri/tauri.intel.conf.json');
  const intel = JSON.parse(fs.readFileSync(intelPath, 'utf8'));
  return {
    baseMinimum: base.bundle?.macOS?.minimumSystemVersion,
    intelMinimum: intel.bundle?.macOS?.minimumSystemVersion,
  };
}

/** Bundled mpv for Intel Vitra requires this macOS major version (Mach-O LC_BUILD_VERSION). */
export const INTEL_MPV_MINIMUM = BUNDLED_MPV['x86_64-apple-darwin'].minimumMacOS;

export function mpvAssetFor(target) {
  return `mpv-${MPV_VERSION}-${BUNDLED_MPV[target]?.asset ?? 'unknown'}.zip`;
}
