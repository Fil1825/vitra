#!/usr/bin/env node
// Decide whether a mingw-w64 crt archive may satisfy a pkg-config -l name.
// An import library is accepted only when every DLL it references is on the
// Windows sidecar allowlist. A crt archive with no DLL and no code is a data
// library (uuid, strmiids) and may be linked as that exact .a file.
// A static third-party archive (text symbols, or a DLL that is not a system
// DLL) is rejected.
// Usage: node windows-crt-link.mjs libfoo.a

import { spawnSync } from 'node:child_process';

import { unexpectedDllImports } from './windows-dll-imports.mjs';

const DLL_NAME = /(?:^|[^A-Za-z0-9_])([A-Za-z0-9_+.-]+\.dll)\b/gi;

export function dllNamesInText(text) {
  return [...new Set([...String(text).matchAll(DLL_NAME)].map((match) => match[1]))];
}

/** True when nm --defined-only shows a global text symbol. */
export function hasDefinedTextSymbol(nmText) {
  return /^[0-9a-fA-F]+\s+[Tt]\s+\S/m.test(String(nmText));
}

/**
 * @returns {{ kind: 'system-import' | 'crt-data' | 'rejected', dlls: string[] }}
 * `dlls` is every referenced DLL for an import library, and the unexpected
 * ones when the archive is rejected for that reason.
 */
export function classifyCrtLibrary({ dllNames, hasText }) {
  const names = dllNames ?? [];
  if (names.length > 0) {
    const unexpected = unexpectedDllImports(names);
    if (unexpected.length === 0) return { kind: 'system-import', dlls: names };
    return { kind: 'rejected', dlls: unexpected };
  }
  if (!hasText) return { kind: 'crt-data', dlls: [] };
  return { kind: 'rejected', dlls: [] };
}

function capture(command, args) {
  const result = spawnSync(command, args, {
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) {
    console.error(`${command} failed: ${result.error.message}`);
    process.exit(1);
  }
  if (result.status !== 0) {
    console.error(`${command} ${args.join(' ')} exited ${result.status}: ${result.stderr.trim()}`);
    process.exit(1);
  }
  return result.stdout;
}

function main() {
  const file = process.argv[2];
  if (!file) {
    console.error('Usage: node windows-crt-link.mjs libfoo.a');
    process.exit(2);
  }
  const strings = capture(process.env.STRINGS || 'strings', ['-a', '-n', '4', file]);
  const nm = capture(process.env.NM || 'nm', ['-g', '--defined-only', file]);
  const verdict = classifyCrtLibrary({
    dllNames: dllNamesInText(strings),
    hasText: hasDefinedTextSymbol(nm),
  });
  if (verdict.kind === 'rejected') {
    const why = verdict.dlls.length
      ? `unexpected DLLs: ${verdict.dlls.join(', ')}`
      : 'static code with no system DLL';
    console.error(`${file}: not a Windows system import library: ${why}`);
    process.exit(1);
  }
  process.stdout.write(`${verdict.kind}\n`);
}

if (process.argv[1] && process.argv[1].endsWith('windows-crt-link.mjs')) main();
