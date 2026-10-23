import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({ message: vi.fn(), open: vi.fn() }));

import { coverKey, isCoverImage, parseCoverKey } from './covers.ts';

describe('covers', () => {
  it('takes the image types a cover can be made of', () => {
    expect(isCoverImage('/Pictures/Front.JPG')).toBe(true);
    expect(isCoverImage('C:\\art\\cover.webp')).toBe(true);
    expect(isCoverImage('/Music/song.flac')).toBe(false);
    expect(isCoverImage('/Pictures/folder')).toBe(false);
  });

  it('names drop targets so they can be read back', () => {
    const key = coverKey({ kind: 'playlist', id: 7 });
    expect(key).toBe('playlist:7');
    expect(parseCoverKey(key)).toEqual({ kind: 'playlist', id: 7 });
    expect(parseCoverKey('track:7')).toBeNull();
    expect(parseCoverKey('album:x')).toBeNull();
  });
});
