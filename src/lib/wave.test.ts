import { describe, expect, it } from 'vitest';

import { sameMode } from './wave.ts';

describe('sameMode', () => {
  it('tells moods and tracks apart', () => {
    expect(sameMode({ kind: 'calm' }, { kind: 'calm' })).toBe(true);
    expect(sameMode({ kind: 'calm' }, { kind: 'sad' })).toBe(false);
    expect(sameMode({ kind: 'likeTrack', track: 3 }, { kind: 'likeTrack', track: 3 })).toBe(true);
    expect(sameMode({ kind: 'likeTrack', track: 3 }, { kind: 'likeTrack', track: 4 })).toBe(false);
    expect(sameMode(null, { kind: 'taste' })).toBe(false);
    expect(sameMode(null, null)).toBe(true);
  });
});
