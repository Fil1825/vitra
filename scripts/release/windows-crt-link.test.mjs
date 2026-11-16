import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { classifyCrtLibrary, dllNamesInText, hasDefinedTextSymbol } from './windows-crt-link.mjs';

const psapi = '/usr/x86_64-w64-mingw32/lib/libpsapi.a';
const haveCrt = fs.existsSync(psapi);

describe('mingw crt link classification', () => {
  it('reads DLL names and text symbols from archive listings', () => {
    expect(dllNamesInText('thunk\nPSAPI.DLL\nAVICAP32.dll\nnot a dll')).toEqual([
      'PSAPI.DLL',
      'AVICAP32.dll',
    ]);
    expect(hasDefinedTextSymbol('0000000000000000 T EnumDeviceDrivers\n')).toBe(true);
    expect(hasDefinedTextSymbol('0000000000000000 R IID_IUnknown\n')).toBe(false);
  });

  it('accepts a system import library and a crt data library, and rejects other code', () => {
    expect(
      classifyCrtLibrary({
        dllNames: ['PSAPI.DLL'],
        hasText: true,
      }).kind,
    ).toBe('system-import');
    expect(
      classifyCrtLibrary({
        dllNames: ['AVICAP32.dll', 'AVIFIL32.dll', 'MSVFW32.dll'],
        hasText: true,
      }).kind,
    ).toBe('system-import');
    expect(classifyCrtLibrary({ dllNames: [], hasText: false }).kind).toBe('crt-data');
    expect(classifyCrtLibrary({ dllNames: [], hasText: true })).toEqual({
      kind: 'rejected',
      dlls: [],
    });
    expect(classifyCrtLibrary({ dllNames: ['libbz2-1.dll'], hasText: true }).dlls).toEqual([
      'libbz2-1.dll',
    ]);
  });

  it.skipIf(!haveCrt)('classifies the real mingw psapi, vfw32, and uuid archives', () => {
    const script = path.join(import.meta.dirname, 'windows-crt-link.mjs');
    const nm = spawnSync('x86_64-w64-mingw32-gcc', ['-print-prog-name=nm'], { encoding: 'utf8' });
    const env = { ...process.env, NM: nm.stdout.trim() || 'nm' };
    const run = (file) => spawnSync(process.execPath, [script, file], { encoding: 'utf8', env });
    expect(run('/usr/x86_64-w64-mingw32/lib/libpsapi.a').stdout).toBe('system-import\n');
    expect(run('/usr/x86_64-w64-mingw32/lib/libvfw32.a').stdout).toBe('system-import\n');
    expect(run('/usr/x86_64-w64-mingw32/lib/libuuid.a').stdout).toBe('crt-data\n');
    expect(run('/usr/x86_64-w64-mingw32/lib/libstrmiids.a').stdout).toBe('crt-data\n');
  });
});
