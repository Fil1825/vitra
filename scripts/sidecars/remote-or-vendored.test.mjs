import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it, vi } from 'vitest';

import { fetchBytesOrVendored } from './remote-or-vendored.mjs';

describe('fetchBytesOrVendored', () => {
  it('returns remote bytes when fetch succeeds', async () => {
    const remote = Buffer.from('remote');
    const bytes = await fetchBytesOrVendored(
      async () => remote,
      'https://example.com/a',
      null,
      '/',
    );
    expect(bytes).toBe(remote);
  });

  it('reads vendored bytes when the remote fetch fails', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-vendored-'));
    const rel = 'checksums/example.txt';
    fs.mkdirSync(path.join(root, path.dirname(rel)), { recursive: true });
    fs.writeFileSync(path.join(root, rel), 'vendored');
    const fallback = vi.fn();
    const bytes = await fetchBytesOrVendored(
      async () => {
        throw new Error('HTTP 504');
      },
      'https://example.com/a',
      rel,
      root,
      fallback,
    );
    expect(bytes.toString()).toBe('vendored');
    expect(fallback).toHaveBeenCalledOnce();
  });

  it('rethrows when no vendored path is configured', async () => {
    await expect(
      fetchBytesOrVendored(async () => {
        throw new Error('HTTP 504');
      }, 'https://example.com/a'),
    ).rejects.toThrow(/HTTP 504/);
  });
});
