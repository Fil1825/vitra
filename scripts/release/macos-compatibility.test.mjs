import { describe, expect, it } from 'vitest';

import {
  BUNDLED_MPV,
  INTEL_MPV_MINIMUM,
  readTauriConfigs,
} from './macos-compatibility.mjs';

describe('macOS packaging compatibility', () => {
  it('keeps the Intel bundle minimum at or below Monterey for unsupported overlay on install', () => {
    const { intelMinimum } = readTauriConfigs();
    expect(intelMinimum).toBe('12.0');
    expect(Number(intelMinimum.split('.')[0])).toBeLessThanOrEqual(12);
  });

  it('does not raise the Intel minimum above the Apple Silicon bundle', () => {
    const { baseMinimum, intelMinimum } = readTauriConfigs();
    expect(Number(intelMinimum.split('.')[0])).toBeLessThanOrEqual(Number(baseMinimum.split('.')[0]));
  });

  it('documents that the bundled Intel mpv itself still requires a newer macOS', () => {
    expect(INTEL_MPV_MINIMUM).toBe(15);
    expect(BUNDLED_MPV['x86_64-apple-darwin'].asset).toBe('macos-15-intel');
  });
});
