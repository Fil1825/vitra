import { describe, expect, it } from 'vitest';

import {
  GITHUB_ACTIONS_CACHE_KEY_PATTERN,
  actionsArchiveCacheKey,
  expectedArchiveDigests,
  sidecarCacheScope,
} from './archive-cache-key.mjs';
import { BTBN_PINS } from './plan.mjs';

describe('archive-cache-key helper', () => {
  it('isolates Linux ffmpeg-only from yt-dlp-only scopes', () => {
    expect(sidecarCacheScope(['ffmpeg'])).toBe('ffmpeg');
    expect(sidecarCacheScope(['yt-dlp'])).toBe('yt-dlp');
    const ffmpegKey = actionsArchiveCacheKey('abc123', 'x86_64-unknown-linux-gnu', 'ffmpeg');
    const ytdlpKey = actionsArchiveCacheKey('abc123', 'x86_64-unknown-linux-gnu', 'yt-dlp');
    expect(ffmpegKey).not.toBe(ytdlpKey);
    expect(ffmpegKey).toBe('sidecar-archives-v3-abc123-x86_64-unknown-linux-gnu-ffmpeg');
    expect(ffmpegKey).toMatch(GITHUB_ACTIONS_CACHE_KEY_PATTERN);
    expect(ffmpegKey).not.toContain(',');
  });

  it('isolates macOS matrix targets', () => {
    const arm = actionsArchiveCacheKey(
      'deadbeef',
      'aarch64-apple-darwin',
      sidecarCacheScope(['yt-dlp', 'ffmpeg']),
    );
    const intel = actionsArchiveCacheKey(
      'deadbeef',
      'x86_64-apple-darwin',
      sidecarCacheScope(['yt-dlp', 'ffmpeg']),
    );
    expect(arm).not.toBe(intel);
    expect(arm).toContain('yt-dlp_ffmpeg');
    expect(arm).not.toContain(',');
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
