#!/usr/bin/env node
// `npm run bundle` runs this before compiling (src-tauri/tauri.bundle.conf.json): it copies the
// fetched sidecars of the target being built into src-tauri/sidecars/bundle/, which
// `bundle.resources` ships as `<resources>/sidecars/` (where src-tauri/src/sidecar.rs looks
// for them). Tauri's config can't name the target triple, hence the fixed folder; and the
// resources stay out of tauri.conf.json because tauri-build copies them on every compile.
//
// Folders (mpv.app, yt-dlp's folder build on macOS) are copied whole. On macOS `ditto`
// keeps their symlinks and extended attributes (code signatures).

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { bundledEntries, hostTarget } from './sidecars/plan.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const target = process.env.TAURI_ENV_TARGET_TRIPLE || hostTarget();
const source = path.join(ROOT, 'src-tauri', 'sidecars', target);
const dest = path.join(ROOT, 'src-tauri', 'sidecars', 'bundle');

fs.rmSync(dest, { recursive: true, force: true });
fs.mkdirSync(dest, { recursive: true });

const names = fs.existsSync(source) ? bundledEntries(fs.readdirSync(source)) : [];
for (const name of names) {
  const from = path.join(source, name);
  const to = path.join(dest, name);
  if (process.platform === 'darwin') {
    const res = spawnSync('ditto', [from, to], { stdio: 'inherit' });
    if (res.status !== 0) {
      console.error(`stage-sidecars: ditto ${from} failed`);
      process.exit(1);
    }
  } else {
    fs.cpSync(from, to, { recursive: true, verbatimSymlinks: true, preserveTimestamps: true });
  }
}

if (names.length === 0) {
  // The bundle still builds; the app then looks for the programs on the system.
  console.warn(`stage-sidecars: nothing in ${path.relative(ROOT, source)}; run fetch-sidecars`);
} else {
  console.log(`stage-sidecars: ${names.join(', ')} → ${path.relative(ROOT, dest)}`);
}
