import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { BTBN_PINS, BTBN_RELEASE_TAG } from './plan.mjs';
import { downloadReleaseAsset } from './github-release.mjs';

const runCold = process.env.VITRA_SIDECAR_COLD_FETCH === '1';

describe.runIf(runCold)('cold upstream fetch (VITRA_SIDECAR_COLD_FETCH=1)', () => {
  it('downloads a pinned BtbN autobuild archive via the documented release-asset API', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-cold-'));
    const dest = path.join(root, BTBN_PINS.linux64.asset);
    const { sha256 } = await downloadReleaseAsset(
      {
        owner: 'BtbN',
        repo: 'FFmpeg-Builds',
        tag: BTBN_RELEASE_TAG,
        asset: BTBN_PINS.linux64.asset,
        assetId: BTBN_PINS.linux64.assetId,
      },
      dest,
    );
    expect(sha256).toBe(BTBN_PINS.linux64.sha256);
  }, 120_000);
});
