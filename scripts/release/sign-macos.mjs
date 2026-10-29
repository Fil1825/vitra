// Sign innermost helpers first; the app envelope is signed later by Tauri.
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function isMachO(bytes) {
  if (bytes.length < 8) return false;
  const magic = bytes.readUInt32BE();
  if ([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe].includes(magic)) return true;
  // Fat Mach-O shares its magic with Java .class; reject impossible architecture counts.
  if ([0xcafebabe, 0xcafebabf].includes(magic))
    return bytes.readUInt32BE(4) > 0 && bytes.readUInt32BE(4) <= 8;
  if ([0xbebafeca, 0xbfbafeca].includes(magic))
    return bytes.readUInt32LE(4) > 0 && bytes.readUInt32LE(4) <= 8;
  return false;
}

export function signingTargets(directory) {
  const files = [],
    bundles = [];
  function visit(current) {
    for (const item of fs.readdirSync(current, { withFileTypes: true })) {
      const name = path.join(current, item.name);
      if (item.isSymbolicLink()) continue;
      if (item.isDirectory()) {
        visit(name);
        if (/\.(app|framework|bundle)$/.test(name)) bundles.push(name);
      } else if (item.isFile()) {
        const fd = fs.openSync(name, 'r');
        try {
          const header = Buffer.alloc(8);
          if (isMachO(header.subarray(0, fs.readSync(fd, header, 0, 8, 0)))) files.push(name);
        } finally {
          fs.closeSync(fd);
        }
      }
    }
  }
  visit(directory);
  const insideFirst = (a, b) =>
    b.split(path.sep).length - a.split(path.sep).length || a.localeCompare(b);
  return [...files.sort(insideFirst), ...bundles.sort(insideFirst)];
}

export function codesignArgs(file, identity, entitlements) {
  const args = ['--force', '--sign', identity];
  if (identity !== '-') args.push('--options', 'runtime', '--timestamp');
  // V8, Python native modules, and media codecs may generate executable code. Reapply
  // the helper entitlements when signing mpv.app's envelope, which signs its main file.
  if (
    ['deno', 'yt-dlp_macos', 'mpv', 'ffmpeg', 'ffprobe'].includes(path.basename(file)) ||
    file.endsWith('.app')
  )
    args.push('--entitlements', entitlements);
  return [...args, file];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.platform !== 'darwin') throw new Error('macOS code signing requires macOS');
  const directory = path.resolve(process.argv[2] ?? '');
  const identity = process.env.APPLE_SIGNING_IDENTITY;
  if (!identity)
    throw new Error('APPLE_SIGNING_IDENTITY is required (use - for local ad-hoc builds)');
  const entitlements = fileURLToPath(new URL('./helper-entitlements.plist', import.meta.url));
  const targets = signingTargets(directory);
  if (!targets.length) throw new Error('No Mach-O helpers to sign');
  for (const file of targets) {
    for (const args of [
      codesignArgs(file, identity, entitlements),
      ['--verify', '--strict', file],
    ]) {
      const result = spawnSync('codesign', args, { stdio: 'inherit' });
      if (result.status !== 0) process.exit(result.status ?? 1);
    }
  }
  console.log(`Signed and verified ${targets.length} helper binaries/bundles`);
}
