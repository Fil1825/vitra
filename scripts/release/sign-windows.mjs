#!/usr/bin/env node
/**
 * Verifies Authenticode on a built NSIS installer when signing was requested.
 * Certificate import is handled in the GitHub workflow (signtool); Tauri runs the sign command.
 */
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

const installer = path.resolve(process.argv[2] ?? '');
if (!fs.existsSync(installer)) {
  console.error('usage: sign-windows.mjs <path-to-setup.exe>');
  process.exit(2);
}

if (process.platform !== 'win32') {
  console.warn('sign-windows: verification intended on Windows');
  process.exit(0);
}

const verify = spawnSync(
  'signtool',
  ['verify', '/pa', '/v', installer],
  { encoding: 'utf8' },
);
if (verify.status !== 0) {
  console.error(verify.stdout);
  console.error(verify.stderr);
  process.exit(verify.status ?? 1);
}
console.log('Authenticode signature verified:', installer);
