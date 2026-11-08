import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { BUNDLE_MINIMUM_MACOS, readTauriConfigs, verifyMachMinOs } from './macos-compatibility.mjs';

describe('macOS packaging compatibility', () => {
  it('matches Tauri plist minimums to bundled sidecar requirements', () => {
    const { armMinimum, intelMinimum } = readTauriConfigs();
    expect(armMinimum).toBe(BUNDLE_MINIMUM_MACOS['aarch64-apple-darwin']);
    expect(intelMinimum).toBe(BUNDLE_MINIMUM_MACOS['x86_64-apple-darwin']);
  });

  it('rejects official Intel mpv below Sequoia (Monterey 12 is not supported)', () => {
    const sidecar = path.join(
      import.meta.dirname,
      '../../src-tauri/sidecars/x86_64-apple-darwin/mpv',
    );
    if (!fs.existsSync(sidecar)) {
      return; // optional in CI until fetch-sidecars runs
    }
    const monterey = verifyMachMinOs(sidecar, '12.7');
    expect(monterey.ok).toBe(false);
    expect(monterey.output).toMatch(/requires macOS 1[45]\./);
  });

  it('mach-minos helper enforces a ceiling on synthetic fixtures', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-mach-'));
    try {
      const exe = path.join(tmp, 'fake');
      // MH_MAGIC_64 + LC_BUILD_VERSION min 15.0.0 at offset 32
      const data = Buffer.alloc(128);
      data.writeUInt32LE(0xfeedfacf, 0);
      data.writeUInt32LE(1, 16); // ncmds
      data.writeUInt32LE(0x32, 32); // LC_BUILD_VERSION
      data.writeUInt32LE(32, 36); // cmdsize
      data.writeUInt32LE(0x0100000c, 40); // platform macOS
      data.writeUInt32LE((15 << 16) | (0 << 8) | 0, 44); // min 15.0.0
      fs.writeFileSync(exe, data);
      expect(verifyMachMinOs(tmp, '12.0').ok).toBe(false);
      expect(verifyMachMinOs(tmp, '15.0').ok).toBe(true);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });
});
