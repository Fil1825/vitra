#!/usr/bin/env node
/**
 * Removes stale absolute RPATH entries (e.g. builder Homebrew paths) from Mach-O files.
 * Requires macOS `install_name_tool`. Safe no-op on other platforms (prints warning).
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const FORBIDDEN = ['/homebrew/', '/opt/homebrew', '/Desktop/homebrew', '/usr/local/opt/'];

function listRpaths(binary) {
  const result = spawnSync('otool', ['-l', binary], { encoding: 'utf8' });
  if (result.status !== 0) return [];
  const rpaths = [];
  const lines = result.stdout.split('\n');
  for (let i = 0; i < lines.length; i++) {
    if (lines[i].trim() === 'cmd LC_RPATH' && i + 2 < lines.length) {
      const pathLine = lines[i + 2].trim();
      const match = pathLine.match(/path\s+(\S+)\s+\(/);
      if (match) rpaths.push(match[1]);
    }
  }
  return rpaths;
}

function stripFile(file, dryRun) {
  const stale = listRpaths(file).filter((p) =>
    FORBIDDEN.some((needle) => p.includes(needle)),
  );
  if (!stale.length) return 0;
  for (const rpath of stale) {
    console.log(`${dryRun ? 'would remove' : 'remove'} rpath ${rpath} from ${file}`);
    if (!dryRun) {
      const result = spawnSync('install_name_tool', ['-delete_rpath', rpath, file], {
        stdio: 'inherit',
      });
      if (result.status !== 0) process.exit(result.status ?? 1);
    }
  }
  return stale.length;
}

function walkMachO(root) {
  const out = [];
  for (const file of fs.readdirSync(root, { withFileTypes: true })) {
    const p = path.join(root, file.name);
    if (file.isDirectory()) out.push(...walkMachO(p));
    else if (
      file.name === 'mpv' ||
      file.suffix === '.dylib' ||
      file.name.endsWith('_macos')
    ) {
      out.push(p);
    }
  }
  return out;
}

const target = path.resolve(process.argv[2] ?? '');
const dryRun = process.argv.includes('--dry-run');
if (!fs.existsSync(target)) {
  console.error('usage: strip-macos-rpaths.mjs <mpv.app or sidecar dir> [--dry-run]');
  process.exit(2);
}

if (process.platform !== 'darwin') {
  console.warn('strip-macos-rpaths: requires macOS (skipping on', process.platform + ')');
  process.exit(0);
}

const root = target.endsWith('.app') ? target : target;
let removed = 0;
for (const file of walkMachO(
  fs.statSync(root).isDirectory() ? root : path.dirname(root),
)) {
  removed += stripFile(file, dryRun);
}
console.log(removed ? `Done (${removed} rpath entries)` : 'No stale rpaths found');
