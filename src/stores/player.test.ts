import { describe, expect, it } from 'vitest';

import { INITIAL_PLAYER_STATE, type PlayerState } from '../lib/player.ts';
import { livePosition } from './player.ts';

const state = (patch: Partial<PlayerState>): PlayerState => ({ ...INITIAL_PLAYER_STATE, ...patch });

describe('livePosition', () => {
  it('advances with time while playing', () => {
    expect(livePosition(state({ status: 'playing', position: 10 }), 1000, 1500)).toBeCloseTo(10.5);
  });

  it('respects the playback speed', () => {
    const s = state({ status: 'playing', position: 10, speed: 2 });
    expect(livePosition(s, 1000, 1500)).toBeCloseTo(11);
  });

  it('stands still when not playing', () => {
    expect(livePosition(state({ status: 'paused', position: 42 }), 0, 99_999)).toBe(42);
  });

  it('never runs past the end', () => {
    const s = state({ status: 'playing', position: 179.9, duration: 180 });
    expect(livePosition(s, 0, 10_000)).toBe(180);
  });

  it('ignores clocks that went backwards', () => {
    expect(livePosition(state({ status: 'playing', position: 5 }), 2000, 1000)).toBe(5);
  });
});
