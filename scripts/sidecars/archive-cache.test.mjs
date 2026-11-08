import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import {
  archiveCachePath,
  readVerifiedArchive,
  sha256File,
  writeVerifiedArchive,
} from './archive-cache.mjs';

const HASH = 'a'.repeat(64);
const OTHER = 'b'.repeat(64);

describe('archive-cache', () => {
  it('writes only when the file matches the expected digest (cold path)', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-cache-'));
    const src = path.join(root, 'src.bin');
    fs.writeFileSync(src, 'hello');
    const expected = sha256File(src);
    writeVerifiedArchive(root, expected, src);
    expect(fs.existsSync(archiveCachePath(root, expected))).toBe(true);
    expect(() => writeVerifiedArchive(root, HASH, src)).toThrow(/refusing to cache/);
  });

  it('reuses a warm entry after re-checking the digest', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-cache-'));
    const src = path.join(root, 'src.bin');
    const dest = path.join(root, 'dest.bin');
    fs.writeFileSync(src, 'payload');
    const expected = sha256File(src);
    writeVerifiedArchive(root, expected, src);
    expect(readVerifiedArchive(root, expected, dest)).toBe(true);
    expect(sha256File(dest)).toBe(expected);
    expect(readVerifiedArchive(root, expected, dest)).toBe(true);
  });

  it('refuses and deletes a poisoned cache entry', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-cache-'));
    const cached = archiveCachePath(root, HASH);
    fs.mkdirSync(path.dirname(cached), { recursive: true });
    fs.writeFileSync(cached, 'tampered');
    const dest = path.join(root, 'out.bin');
    expect(() => readVerifiedArchive(root, HASH, dest)).toThrow(/poisoned archive cache/);
    expect(fs.existsSync(cached)).toBe(false);
    expect(readVerifiedArchive(root, OTHER, dest)).toBe(false);
  });
});
