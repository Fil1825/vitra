#!/usr/bin/env node
// Rewrite pkg-config Libs lines so the Windows sidecar link uses a static
// archive (.a) instead of an MSYS2 import library (.dll.a). A -l name that is
// neither one of our archives nor a Windows system import library fails the
// build. That is earlier than the DLL import check, and it names the library.
// Usage: node windows-pkgconfig-static.mjs FILE.pc libfoo.a libz.a ...

import fs from 'node:fs';

/**
 * Link names of Windows system import libraries. These are not MSYS2
 * packages. Matched case-insensitively. The DLL import check is still the
 * gate for what the executable is allowed to load.
 */
export const WINDOWS_SYSTEM_LINK_NAMES = new Set(
  [
    'advapi32',
    'avicap32',
    'avrt',
    'bcrypt',
    'bcryptprimitives',
    'cfgmgr32',
    'comctl32',
    'comdlg32',
    'crypt32',
    'd3d11',
    'd3d9',
    'd3dcompiler',
    'd3dcompiler_47',
    'dnsapi',
    'dwmapi',
    'dxgi',
    'dxva2',
    'gdi32',
    'gdiplus',
    'glu32',
    'imm32',
    'iphlpapi',
    'kernel32',
    'ksuser',
    'mf',
    'mfplat',
    'mfreadwrite',
    'mpr',
    'msimg32',
    'msvcrt',
    'ntdll',
    'ole32',
    'oleacc',
    'oleaut32',
    'opengl32',
    'pathcch',
    'powrprof',
    'propsys',
    'rpcrt4',
    'secur32',
    'setupapi',
    'shcore',
    'shell32',
    'shlwapi',
    'strmiids',
    'user32',
    'userenv',
    'uuid',
    'uxtheme',
    'version',
    'winhttp',
    'wininet',
    'winmm',
    'winspool',
    'ws2_32',
    'wtsapi32',
  ].map((name) => name.toLowerCase()),
);

export function staticLinkNames(archiveNames) {
  const names = new Set();
  for (const archive of archiveNames) {
    const base = archive.split(/[\\/]/).pop();
    const match = /^lib(.+)\.a$/i.exec(base);
    if (!match || base.toLowerCase().endsWith('.dll.a')) continue;
    names.add(match[1].toLowerCase());
  }
  return names;
}

function rewriteFlag(token, staticLibs, missing) {
  const exact = /^-l:lib(.+)\.a$/i.exec(token);
  if (exact) {
    const name = exact[1].toLowerCase();
    if (!staticLibs.has(name)) missing.push(token);
    return token;
  }
  const shared = /^-l(.+)\.dll$/i.exec(token);
  if (shared) {
    missing.push(token);
    return token;
  }
  const plain = /^-l(?!:)(.+)$/i.exec(token);
  if (!plain) return token;
  const name = plain[1].toLowerCase();
  if (staticLibs.has(name)) return `-l:lib${plain[1]}.a`;
  if (WINDOWS_SYSTEM_LINK_NAMES.has(name)) return token;
  missing.push(token);
  return token;
}

/** Rewrite Libs and Libs.private. Throws when a -l flag has no static archive. */
export function rewritePkgconfigLibs(text, archiveNames) {
  const staticLibs = staticLinkNames(archiveNames);
  const missing = [];
  const rewritten = String(text).replace(/^Libs(?:\.private)?:.*$/gm, (line) =>
    line.replace(/(^|\s)(-l\S+)/g, (full, lead, flag) => {
      return lead + rewriteFlag(flag, staticLibs, missing);
    }),
  );
  if (missing.length > 0) {
    throw new Error(
      `pkg-config would link a non-static library: ${[...new Set(missing)].join(', ')}`,
    );
  }
  return rewritten;
}

function main() {
  const [file, ...archives] = process.argv.slice(2);
  if (!file) {
    console.error('Usage: node windows-pkgconfig-static.mjs FILE.pc libfoo.a ...');
    process.exit(2);
  }
  const original = fs.readFileSync(file, 'utf8');
  let rewritten;
  try {
    rewritten = rewritePkgconfigLibs(original, archives);
  } catch (error) {
    console.error(`${file}: ${error.message}`);
    process.exit(1);
  }
  if (rewritten !== original) fs.writeFileSync(file, rewritten);
}

if (process.argv[1] && process.argv[1].endsWith('windows-pkgconfig-static.mjs')) main();
