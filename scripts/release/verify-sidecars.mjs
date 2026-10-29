import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { assertSidecars, requiredSidecars } from './sidecars.mjs';

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
    const result = spawnSync(path.join(directory, program), [ffmpeg ? '-version' : '--version'], {
      encoding: 'utf8',
      timeout: 120_000,
    });
    if (result.status !== 0)
      throw new Error(
        `Packaged helper failed: ${program}: ${result.error?.message ?? result.stderr}`,
      );
    console.log(`${program}: ${(result.stdout || result.stderr).split('\n')[0]}`);
  }
}
