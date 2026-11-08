import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  BUNDLE_MINIMUM_MACOS,
  readTauriConfigs,
  verifyMachMinOs,
  verifyMpvMachDeps,
} from './macos-compatibility.mjs';

const INTEL_MPV = path.join(
  path.resolve(import.meta.dirname, '../..'),
  'src-tauri/sidecars/x86_64-apple-darwin/mpv',
);
const INTEL_MPV_APP = path.join(INTEL_MPV, 'mpv.app');

describe('macOS packaging compatibility', () => {
  it('matches Tauri plist minimums to bundled sidecar requirements', () => {
    const { armMinimum, intelMinimum } = readTauriConfigs();
    expect(armMinimum).toBe(BUNDLE_MINIMUM_MACOS['aarch64-apple-darwin']);
    expect(intelMinimum).toBe(BUNDLE_MINIMUM_MACOS['x86_64-apple-darwin']);
  });

  describe.skipIf(!fs.existsSync(path.join(INTEL_MPV_APP, 'Contents/MacOS/mpv')))(
    'Intel mpv sidecar (only when a source build is already present)',
    () => {
      it('passes Monterey Mach-O ceiling', () => {
        expect(fs.existsSync(INTEL_MPV)).toBe(true);
        const monterey = verifyMachMinOs(INTEL_MPV, '12.7');
        expect(monterey.ok).toBe(true);
      });

      it('resolves bundled dylibs without Homebrew load paths', () => {
        const deps = verifyMpvMachDeps(INTEL_MPV_APP);
        expect(deps.ok).toBe(true);
        expect(deps.output).toMatch(/^LOAD\t@executable_path\/lib\//m);
        expect(deps.output).not.toMatch(/LOAD\t\/opt\/homebrew/);
      });
    },
  );

  it('mach-minos helper enforces a ceiling on synthetic fixtures', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-mach-'));
    try {
      const exe = path.join(tmp, 'fake');
      const data = Buffer.alloc(128);
      data.writeUInt32LE(0xfeedfacf, 0);
      data.writeUInt32LE(1, 16);
      data.writeUInt32LE(0x32, 32);
      data.writeUInt32LE(32, 36);
      data.writeUInt32LE(0x0100000c, 40);
      data.writeUInt32LE((15 << 16) | (0 << 8) | 0, 44);
      fs.writeFileSync(exe, data);
      expect(verifyMachMinOs(tmp, '12.0').ok).toBe(false);
      expect(verifyMachMinOs(tmp, '15.0').ok).toBe(true);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });
});
