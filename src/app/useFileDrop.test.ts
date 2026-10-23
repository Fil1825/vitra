import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({ message: vi.fn(), open: vi.fn() }));

import { coverAt, dropPoint, isPlayableDrop } from './useFileDrop.ts';

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

describe('drop positions', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('are logical on macOS and Linux, physical on Windows', () => {
    const position = { x: 300, y: 120 };
    expect(dropPoint(position, 'macos', 2)).toEqual({ x: 300, y: 120 });
    expect(dropPoint(position, 'linux', 2)).toEqual({ x: 300, y: 120 });
    expect(dropPoint(position, 'windows', 1.5)).toEqual({ x: 200, y: 80 });
    expect(dropPoint(position, 'windows', 0)).toEqual({ x: 300, y: 120 });
  });

  it('find the cover under them', () => {
    const elementFromPoint = vi.fn((x: number, y: number) => ({
      closest: () => (x > 250 ? { dataset: { coverDrop: `album:${y}` } } : null),
    }));
    vi.stubGlobal('document', { elementFromPoint });
    // A Retina Mac: the point is where the cover is, not half of it.
    expect(coverAt({ x: 300, y: 120 }, 'macos', 2)).toEqual({ kind: 'album', id: 120 });
    expect(elementFromPoint).toHaveBeenLastCalledWith(300, 120);
    // Windows at 200 %: halved, and off the cover.
    expect(coverAt({ x: 300, y: 120 }, 'windows', 2)).toBeNull();
    expect(elementFromPoint).toHaveBeenLastCalledWith(150, 60);
  });
});
