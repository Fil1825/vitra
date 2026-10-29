import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { codesignArgs, isMachO, signingTargets } from './sign-macos.mjs';

describe('nested macOS code signatures', () => {
  it('recognizes native/universal executables without treating Java classes as Mach-O', () => {
    for (const magic of [0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe]) {
      const bytes = Buffer.alloc(8);
      bytes.writeUInt32BE(magic);
      expect(isMachO(bytes)).toBe(true);
    }
    const fat = Buffer.from('cafebabe00000002', 'hex');
    expect(isMachO(fat)).toBe(true);
    expect(isMachO(Buffer.from('cafebabe00000041', 'hex'))).toBe(false);
    expect(isMachO(Buffer.from('cafebabe00000034', 'hex'))).toBe(false);
    expect(isMachO(Buffer.from('not code'))).toBe(false);
  });

  it('signs nested Mach-O files before their app without following aliases', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-sign-'));
    try {
      const bundle = path.join(root, 'mpv.app');
      const executable = path.join(bundle, 'Contents', 'MacOS', 'mpv');
      const dylib = path.join(bundle, 'Contents', 'MacOS', 'lib', 'audio.dylib');
      fs.mkdirSync(path.dirname(dylib), { recursive: true });
      for (const file of [executable, dylib])
        fs.writeFileSync(file, Buffer.from('cffaedfe00000000', 'hex'));
      fs.writeFileSync(path.join(root, 'README'), 'data');
      if (process.platform !== 'win32') fs.symlinkSync(dylib, path.join(root, 'alias'));
      expect(signingTargets(root)).toEqual([dylib, executable, bundle]);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it('uses hardened runtime and secure timestamps only for Developer ID signatures', () => {
    const signed = codesignArgs('/helpers/deno', 'Developer ID Application: test', '/jit.plist');
    expect(signed).toContain('--timestamp');
    expect(signed).toContain('runtime');
    expect(signed).toContain('/jit.plist');
    expect(signed).not.toContain('--deep');
    expect(codesignArgs('/helpers/ffmpeg', '-', '/jit.plist')).not.toContain('--timestamp');
    expect(codesignArgs('/helpers/libavcodec.dylib', '-', '/jit.plist')).not.toContain(
      '--entitlements',
    );
    expect(codesignArgs('/helpers/mpv.app', '-', '/jit.plist')).toContain('/jit.plist');
  });
});
