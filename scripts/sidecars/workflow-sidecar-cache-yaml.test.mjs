import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  extractSidecarCachePairs,
  validateWorkflowSidecarCaches,
} from './parse-workflow-sidecar-cache.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const CI_YML = path.join(ROOT, '.github/workflows/ci.yml');
const RELEASE_YML = path.join(ROOT, '.github/workflows/release.yml');

describe('workflow YAML sidecar cache keys (read from disk)', () => {
  it('validates ci.yml restore keys against the following fetch-sidecars step', () => {
    const text = fs.readFileSync(CI_YML, 'utf8');
    const pairs = validateWorkflowSidecarCaches(text, '.github/workflows/ci.yml');
    expect(pairs).toHaveLength(4);
  });

  it('validates release.yml matrix restore keys against fetch-sidecars', () => {
    const text = fs.readFileSync(RELEASE_YML, 'utf8');
    const pairs = validateWorkflowSidecarCaches(text, '.github/workflows/release.yml');
    expect(pairs).toHaveLength(1);
    expect(pairs[0].keySuffix).toContain('${{ matrix.target }}');
    expect(pairs[0].keySuffix).not.toContain(',');
  });

  it('fails when a workflow key suffix reintroduces a comma (original Actions bug)', () => {
    const real = fs.readFileSync(CI_YML, 'utf8');
    const pairs = extractSidecarCachePairs(real);
    const macos = pairs.find((p) => p.keySuffix.startsWith('aarch64-apple-darwin-'));
    expect(macos?.keySuffix).toBe('aarch64-apple-darwin-yt-dlp_ffmpeg');
    const corrupted = real.replace(
      'aarch64-apple-darwin-yt-dlp_ffmpeg',
      'aarch64-apple-darwin-yt-dlp,ffmpeg',
    );
    expect(() => validateWorkflowSidecarCaches(corrupted, 'synthetic-ci.yml')).toThrow(
      /comma|yt-dlp,ffmpeg/,
    );
  });

  it('fails when cache key scope drifts from --only fetch flags', () => {
    const real = fs.readFileSync(CI_YML, 'utf8');
    const drifted = real.replace(
      '-x86_64-unknown-linux-gnu-yt-dlp',
      '-x86_64-unknown-linux-gnu-ffmpeg',
    );
    expect(() => validateWorkflowSidecarCaches(drifted, 'synthetic-ci.yml')).toThrow(
      /does not match fetch scope/,
    );
  });
});
