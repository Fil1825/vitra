#!/usr/bin/env node
// Allow-list of DLL imports for the Windows sidecar executables.
// Anything else is a library we must link statically or ship, with its license.
// Usage: objdump -p PROGRAM | node scripts/release/windows-dll-imports.mjs PROGRAM

import fs from 'node:fs';

/** Windows and API-set libraries. Matched case-insensitively. */
export const WINDOWS_SYSTEM_DLLS = new Set(
  [
    'advapi32.dll',
    'avrt.dll',
    'bcrypt.dll',
    'bcryptprimitives.dll',
    'cfgmgr32.dll',
    'comctl32.dll',
    'comdlg32.dll',
    'crypt32.dll',
    'cryptbase.dll',
    'd3d11.dll',
    'd3d9.dll',
    'd3dcompiler_47.dll',
    'dnsapi.dll',
    'dwmapi.dll',
    'dxgi.dll',
    'dxva2.dll',
    'gdi32.dll',
    'gdiplus.dll',
    'glu32.dll',
    'imm32.dll',
    'iphlpapi.dll',
    'kernel32.dll',
    'ksuser.dll',
    'mf.dll',
    'mfplat.dll',
    'mfreadwrite.dll',
    'mpr.dll',
    'msimg32.dll',
    'msvcrt.dll',
    'ntdll.dll',
    'ole32.dll',
    'oleacc.dll',
    'oleaut32.dll',
    'opengl32.dll',
    'powrprof.dll',
    'propsys.dll',
    'rpcrt4.dll',
    'secur32.dll',
    'setupapi.dll',
    // Avicap32.dll is Video for Windows (capCreateCaptureWindow). Microsoft
    // documents the minimum client as Windows 2000 Professional, and the DLL
    // still ships in System32. Vitra's Windows builds target Windows 10/11.
    // mpv links it for DirectShow.
    'avicap32.dll',
    'shell32.dll',
    // Shcore.dll is SetProcessDpiAwareness (shellscalingapi.h). Microsoft
    // documents the minimum client as Windows 8.1. Windows 10 and 11 include
    // it in System32, which is Vitra's supported Windows baseline.
    'shcore.dll',
    'shlwapi.dll',
    'user32.dll',
    'userenv.dll',
    'uxtheme.dll',
    'version.dll',
    'winhttp.dll',
    'wininet.dll',
    'winmm.dll',
    'winspool.drv',
    'ws2_32.dll',
    'wtsapi32.dll',
  ].map((name) => name.toLowerCase()),
);

const SYSTEM_DLL_PREFIXES = ['api-ms-win-', 'ext-ms-win-'];

export function dllImports(objdump) {
  return [...String(objdump).matchAll(/DLL Name:\s*(\S+)/gi)].map((match) => match[1]);
}

export function unexpectedDllImports(names) {
  return names.filter((name) => {
    const lower = name.toLowerCase();
    if (WINDOWS_SYSTEM_DLLS.has(lower)) return false;
    return !SYSTEM_DLL_PREFIXES.some((prefix) => lower.startsWith(prefix));
  });
}

function main() {
  const exe = process.argv[2];
  if (!exe) {
    console.error('Usage: objdump -p PROGRAM | node windows-dll-imports.mjs PROGRAM');
    process.exit(2);
  }
  const names = dllImports(fs.readFileSync(0, 'utf8'));
  if (names.length === 0) {
    console.error(`${exe} has no DLL imports; objdump did not show a PE import table`);
    process.exit(1);
  }
  const unexpected = unexpectedDllImports(names);
  if (unexpected.length > 0) {
    console.error(`${exe} imports non-system DLLs: ${unexpected.join(', ')}`);
    console.error(`All imports: ${names.join(', ')}`);
    process.exit(1);
  }
}

if (process.argv[1] && process.argv[1].endsWith('windows-dll-imports.mjs')) main();
