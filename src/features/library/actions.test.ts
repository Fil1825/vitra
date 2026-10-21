import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/plugin-dialog', () => ({ message: vi.fn(), open: vi.fn() }));

const { containingFolder } = await import('./actions.ts');

describe('containingFolder', () => {
  const folders = ['/music', 'C:\\Users\\me\\Music'];

  it('finds the library folder a path is inside', () => {
    expect(containingFolder('/music/rock', folders)).toBe('/music');
    expect(containingFolder('/music/rock/', folders)).toBe('/music');
    expect(containingFolder('C:\\Users\\me\\Music\\Jazz', folders)).toBe('C:\\Users\\me\\Music');
  });

  it('ignores the folder itself and lookalike siblings', () => {
    expect(containingFolder('/music', folders)).toBeUndefined();
    expect(containingFolder('/musical', folders)).toBeUndefined();
    expect(containingFolder('/other', folders)).toBeUndefined();
  });
});
