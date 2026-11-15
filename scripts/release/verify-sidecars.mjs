import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { assertSidecars, requiredSidecars } from './sidecars.mjs';

function quoteCmd(value) {
  return `"${String(value).replaceAll('"', '""')}"`;
}

/** Run a packaged helper. Windows CreateProcess runs `ffmpeg.exe` directly.
 * A `.cmd` launcher (used by the unit fake) goes through cmd.exe. */
function spawnPackaged(program, args) {
  const options = { encoding: 'utf8', timeout: 120_000, windowsHide: true };
  if (process.platform === 'win32' && /\.(?:cmd|bat)$/i.test(program)) {
    const command = [quoteCmd(program), ...args.map(quoteCmd)].join(' ');
    return spawnSync(process.env.ComSpec || 'cmd.exe', ['/d', '/s', '/c', `"${command}"`], {
      ...options,
      windowsVerbatimArguments: true,
    });
  }
  return spawnSync(program, args, options);
}

/** The packaged ffmpeg must be able to encode MP3. yt-dlp asks for this encoder by name. */
export function assertLibmp3lameEncoder(program) {
  const result = spawnPackaged(program, ['-hide_banner', '-encoders']);
  if (result.error || result.status === null) {
    const reason = result.error?.message ?? 'ffmpeg exited without a status';
    throw new Error(`Packaged ffmpeg could not be started: ${program}: ${reason}`);
  }
  const text = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  if (result.status !== 0 || !/(^|\s)libmp3lame(\s|$)/m.test(text)) {
    throw new Error(`Packaged ffmpeg is missing the libmp3lame encoder: ${program}`);
  }
}

/** Find the resource directory in an extracted/installed package, never in the source tree. */
export function findSidecars(root, target) {
  const found = [];
  function visit(directory) {
    if (path.basename(directory) === 'sidecars') {
      try {
        assertSidecars(directory, target);
        found.push(directory);
      } catch {
        /* A partial candidate isn't a package. */
      }
    }
    for (const item of fs.readdirSync(directory, { withFileTypes: true })) {
      if (item.isDirectory() && !item.isSymbolicLink()) visit(path.join(directory, item.name));
    }
  }
  visit(root);
  if (found.length !== 1)
    throw new Error(`Expected one complete packaged sidecar directory, found ${found.length}`);
  return found[0];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [root, target] = process.argv.slice(2);
  if (!root || !target)
    throw new Error('Usage: node scripts/release/verify-sidecars.mjs PACKAGE_ROOT TARGET');
  const directory = findSidecars(path.resolve(root), target);
  for (const program of requiredSidecars(target)) {
    const ffmpeg = /^ff(mpeg|probe)/.test(path.basename(program));
    const result = spawnPackaged(path.join(directory, program), [
      ffmpeg ? '-version' : '--version',
    ]);
    if (result.error || result.status !== 0)
      throw new Error(
        `Packaged helper failed: ${program}: ${result.error?.message ?? result.stderr}`,
      );
    const base = path.basename(program);
    if (base === 'ffmpeg' || base === 'ffmpeg.exe')
      assertLibmp3lameEncoder(path.join(directory, program));
    console.log(`${program}: ${(result.stdout || result.stderr).split('\n')[0]}`);
  }
}
