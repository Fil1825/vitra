import { describe, expect, it } from 'vitest';

import { dllImports, unexpectedDllImports } from './windows-dll-imports.mjs';

describe('Windows sidecar DLL imports', () => {
  it('reads DLL Name lines and rejects a loader that is not a system library', () => {
    const dump = `
	DLL Name: KERNEL32.dll
	DLL Name: msvcrt.dll
	DLL Name: vulkan-1.dll
	DLL Name: api-ms-win-crt-runtime-l1-1-0.dll
`;
    expect(dllImports(dump)).toEqual([
      'KERNEL32.dll',
      'msvcrt.dll',
      'vulkan-1.dll',
      'api-ms-win-crt-runtime-l1-1-0.dll',
    ]);
    expect(unexpectedDllImports(dllImports(dump))).toEqual(['vulkan-1.dll']);
  });

  it('allows Shcore and Avicap32, which ship with Windows 10', () => {
    expect(unexpectedDllImports(['SHCORE.dll', 'AVICAP32.dll', 'KERNEL32.dll'])).toEqual([]);
  });

  it('allows the system DLLs behind the mingw psapi and vfw32 import libraries', () => {
    expect(
      unexpectedDllImports(['PSAPI.DLL', 'AVIFIL32.dll', 'MSVFW32.dll', 'AVICAP32.dll']),
    ).toEqual([]);
  });

  it('rejects MinGW runtime and shared codec libraries', () => {
    expect(
      unexpectedDllImports([
        'libstdc++-6.dll',
        'libgcc_s_seh-1.dll',
        'libwinpthread-1.dll',
        'shaderc_shared.dll',
        'SDL2.dll',
        'libass-9.dll',
        'libiconv-2.dll',
        'zlib1.dll',
      ]),
    ).toEqual([
      'libstdc++-6.dll',
      'libgcc_s_seh-1.dll',
      'libwinpthread-1.dll',
      'shaderc_shared.dll',
      'SDL2.dll',
      'libass-9.dll',
      'libiconv-2.dll',
      'zlib1.dll',
    ]);
  });
});
