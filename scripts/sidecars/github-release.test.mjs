import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { createHash } from 'node:crypto';

import { describe, expect, it } from 'vitest';

import { downloadReleaseAsset, resolveReleaseAsset } from './github-release.mjs';

const fixture = {
  owner: 'example',
  repo: 'widgets',
  tag: 'v1.0.0',
  asset: 'widget.zip',
  assetId: 4242,
};

function mockFetch(handlers) {
  return async (url, init) => {
    const key = `${init?.headers?.Accept ?? ''} ${url}`;
    const handler = handlers[key] ?? handlers[url];
    if (!handler) {
      throw new Error(`unexpected fetch ${url}`);
    }
    return handler(url, init);
  };
}

describe('github-release', () => {
  it('resolves asset IDs from the release-by-tag metadata', async () => {
    const fetchFn = mockFetch({
      'application/vnd.github+json https://api.github.com/repos/example/widgets/releases/tags/v1.0.0':
        async () => ({
          ok: true,
          async json() {
            return { assets: [{ id: 4242, name: 'widget.zip' }] };
          },
        }),
    });
    await expect(resolveReleaseAsset(fixture, fetchFn)).resolves.toEqual({
      assetId: 4242,
      name: 'widget.zip',
    });
  });

  it('downloads via application/octet-stream and returns a digest', async () => {
    const body = Buffer.from('zip-bytes');
    const fetchFn = mockFetch({
      'application/octet-stream https://api.github.com/repos/example/widgets/releases/assets/4242':
        async () => ({
          ok: true,
          url: 'https://example.com/expiring-signed-url',
          body: new ReadableStream({
            start(controller) {
              controller.enqueue(body);
              controller.close();
            },
          }),
        }),
    });
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-gh-'));
    const dest = path.join(root, 'widget.zip');
    const result = await downloadReleaseAsset({ ...fixture, assetId: 4242 }, dest, fetchFn);
    expect(result.provenance).toBe('github-api:example/widgets@v1.0.0/widget.zip');
    expect(result.sha256).toBe(createHash('sha256').update(body).digest('hex'));
    expect(fs.readFileSync(dest).equals(body)).toBe(true);
  });
});
