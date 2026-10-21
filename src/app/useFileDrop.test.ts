import { describe, expect, it } from 'vitest';

import { isPlayableDrop } from './useFileDrop.ts';

describe('isPlayableDrop', () => {
  it('accepts audio files and likely folders', () => {
    expect(isPlayableDrop('/music/a.FLAC')).toBe(true);
    expect(isPlayableDrop('C:\\Music\\Albums')).toBe(true);
    expect(isPlayableDrop('/music/Album')).toBe(true);
  });

  it('rejects other files', () => {
    expect(isPlayableDrop('/music/cover.jpg')).toBe(false);
    expect(isPlayableDrop('/music/notes.txt')).toBe(false);
  });
});
