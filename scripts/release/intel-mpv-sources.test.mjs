import fs from 'node:fs';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { planFor } from '../sidecars/plan.mjs';

const pins = JSON.parse(
  fs.readFileSync(path.join(import.meta.dirname, 'intel-mpv-sources.json'), 'utf8'),
);
const recipe = fs.readFileSync(path.join(import.meta.dirname, 'build-intel-mpv.sh'), 'utf8');

describe('Intel mpv source pins', () => {
  it('targets Monterey and pins every archive', () => {
    expect(pins.deploymentTarget).toBe('12.0');
    expect(pins.arch).toBe('x86_64');
    expect(pins.mpv.sha256).toMatch(/^[0-9a-f]{64}$/);
    expect(pins.mpv.url).not.toMatch(/stolendata/);
    for (const dep of pins.dependencies) {
      expect(dep.sha256).toMatch(/^[0-9a-f]{64}$/);
      expect(dep.url).not.toMatch(/brew|stolendata/);
      for (const sub of dep.submodules ?? []) {
        expect(sub.commit).toMatch(/^[0-9a-f]{40}$/);
        expect(sub.sha256).toMatch(/^[0-9a-f]{64}$/);
      }
    }
  });

  it('keeps the recipe fail-closed on checksums and min OS', () => {
    expect(recipe).toContain('MACOSX_DEPLOYMENT_TARGET=12.0');
    expect(recipe).toContain('checksum mismatch');
    expect(recipe).toContain('--max 12.0');
    expect(recipe).not.toContain('stolendata');
    expect(planFor('x86_64-apple-darwin').mpv.sources).toBe(
      'scripts/release/intel-mpv-sources.json',
    );
  });
});
