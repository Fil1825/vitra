#!/usr/bin/env node
/**
 * Verifies Authenticode on a built NSIS installer when signing was requested.
 * Requires Windows SDK signtool (present on windows-2022 runners with VS toolchain).
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

function resolveSigntool() {
  if (process.platform !== 'win32') {
    return null;
  }
  const lookup = spawnSync('where.exe', ['signtool'], { encoding: 'utf8' });
  if (lookup.status === 0) {
    const line = lookup.stdout.trim().split(/\r?\n/).find(Boolean);
    if (line) return line.trim();
  }
  const kitsRoot = process.env['ProgramFiles(x86)'] + '\\Windows Kits\\10\\bin';
  if (fs.existsSync(kitsRoot)) {
    const versions = fs
      .readdirSync(kitsRoot)
      .filter((d) => /^\d+\.\d+\.\d+\.\d+$/.test(d))
      .sort()
      .reverse();
    for (const ver of versions) {
      for (const arch of ['x64', 'x86', 'arm64']) {
        const candidate = path.join(kitsRoot, ver, arch, 'signtool.exe');
        if (fs.existsSync(candidate)) return candidate;
      }
    }
  }
  return null;
}

const installer = path.resolve(process.argv[2] ?? '');
if (!fs.existsSync(installer)) {
  console.error('usage: sign-windows.mjs <path-to-setup.exe>');
  process.exit(2);
}

if (process.platform !== 'win32') {
  console.warn('sign-windows: verification intended on Windows');
  process.exit(0);
}

const signtool = resolveSigntool();
if (!signtool) {
  console.error('signtool not found; install Windows SDK signing tools on the runner');
  process.exit(1);
}

const verify = spawnSync(signtool, ['verify', '/pa', '/v', installer], { encoding: 'utf8' });
if (verify.status !== 0) {
  console.error(verify.stdout);
  console.error(verify.stderr);
  process.exit(verify.status ?? 1);
}
console.log('Authenticode signature verified:', installer);
