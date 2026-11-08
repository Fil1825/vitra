import { describe, expect, it } from 'vitest';

import {
  actionsArchiveCacheKey,
  expectedArchiveDigests,
  sidecarCacheScope,
} from './archive-cache-key.mjs';
import { BTBN_PINS } from './plan.mjs';

describe('archive-cache-key', () => {
  it('isolates Linux ffmpeg-only from yt-dlp-only scopes', () => {
    expect(sidecarCacheScope(['ffmpeg'])).toBe('ffmpeg');
    expect(sidecarCacheScope(['yt-dlp'])).toBe('yt-dlp');
    const manifest = 'abc123';
    const target = 'x86_64-unknown-linux-gnu';
    const ffmpegKey = actionsArchiveCacheKey(manifest, target, 'ffmpeg');
    const ytdlpKey = actionsArchiveCacheKey(manifest, target, 'yt-dlp');
    expect(ffmpegKey).not.toBe(ytdlpKey);
    expect(ffmpegKey).toBe('sidecar-archives-v2-abc123-x86_64-unknown-linux-gnu-ffmpeg');
  });

  it('isolates macOS matrix targets', () => {
    const manifest = 'deadbeef';
    const arm = actionsArchiveCacheKey(manifest, 'aarch64-apple-darwin', 'yt-dlp,ffmpeg');
    const intel = actionsArchiveCacheKey(manifest, 'x86_64-apple-darwin', 'yt-dlp,ffmpeg');
    expect(arm).not.toBe(intel);
  });

  it('lists pinned digests for a scoped linux ffmpeg fetch', () => {
    const digests = expectedArchiveDigests('x86_64-unknown-linux-gnu', ['ffmpeg']);
    expect(digests).toEqual([
      {
        sidecar: 'ffmpeg',
        name: BTBN_PINS.linux64.asset,
        sha256: BTBN_PINS.linux64.sha256,
      },
    ]);
  });
});
