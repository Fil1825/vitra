import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

/** Content-addressed store; trusted digest always comes from the sidecar plan, never from cache metadata. */
export function defaultArchiveCacheRoot() {
  const home = process.env.HOME ?? process.env.USERPROFILE;
  if (!home) {
    return path.join(process.cwd(), '.cache', 'vitra-sidecar-archives');
  }
  return path.join(home, '.cache', 'vitra-sidecar-archives');
}

export function archiveCachePath(cacheRoot, expectedSha256) {
  const hex = expectedSha256.toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(hex)) {
    throw new Error(`invalid expected sha256 for cache: ${expectedSha256}`);
  }
  return path.join(cacheRoot, hex.slice(0, 2), hex, 'archive.bin');
}

export function sha256File(file) {
  const hash = createHash('sha256');
  const fd = fs.openSync(file, 'r');
  try {
    const buf = Buffer.alloc(1024 * 1024);
    let n;
    while ((n = fs.readSync(fd, buf, 0, buf.length, null)) > 0) {
      hash.update(buf.subarray(0, n));
    }
  } finally {
    fs.closeSync(fd);
  }
  return hash.digest('hex');
}

/**
 * Copies a cached archive when its on-disk digest matches `expectedSha256`.
 * Deletes and throws on mismatch (poisoned entry).
 */
export function readVerifiedArchive(cacheRoot, expectedSha256, destFile) {
  const cached = archiveCachePath(cacheRoot, expectedSha256);
  if (!fs.existsSync(cached)) {
    return false;
  }
  const actual = sha256File(cached);
  const expected = expectedSha256.toLowerCase();
  if (actual !== expected) {
    fs.rmSync(path.join(cacheRoot, expected.slice(0, 2), expected), {
      recursive: true,
      force: true,
    });
    throw new Error(`refusing poisoned archive cache (expected ${expected}, found ${actual})`);
  }
  fs.mkdirSync(path.dirname(destFile), { recursive: true });
  fs.copyFileSync(cached, destFile);
  return true;
}

/** Writes only when the file already matches the pinned digest from the plan. */
export function writeVerifiedArchive(cacheRoot, expectedSha256, sourceFile) {
  const actual = sha256File(sourceFile);
  const expected = expectedSha256.toLowerCase();
  if (actual !== expected) {
    throw new Error(`refusing to cache archive: expected ${expected}, got ${actual}`);
  }
  const dest = archiveCachePath(cacheRoot, expected);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.copyFileSync(sourceFile, dest);
}
