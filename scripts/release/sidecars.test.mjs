import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { assertSidecars, requiredSidecars } from './sidecars.mjs';

describe('distributable sidecars', () => {
  it('requires platform layouts, including macOS folder builds', () => {
    expect(requiredSidecars('aarch64-apple-darwin')).toContain('yt-dlp/yt-dlp_macos');
    expect(requiredSidecars('aarch64-apple-darwin')).toContain('mpv/mpv.app/Contents/MacOS/mpv');
    expect(requiredSidecars('x86_64-pc-windows-msvc')).toContain('mpv/mpv.exe');
    expect(requiredSidecars('x86_64-unknown-linux-gnu')).not.toContain('mpv');
    expect(() => requiredSidecars('unknown')).toThrow('Unsupported');
  });

  it('rejects partial fetches instead of shipping an application that cannot play', () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-bundle-'));
    try {
      const target = 'x86_64-unknown-linux-gnu';
      expect(() => assertSidecars(directory, target)).toThrow('Incomplete sidecars');
      for (const name of requiredSidecars(target))
        fs.writeFileSync(path.join(directory, name), 'test program');
      expect(() => assertSidecars(directory, target)).not.toThrow();
      fs.unlinkSync(path.join(directory, 'deno'));
      fs.mkdirSync(path.join(directory, 'deno'));
      expect(() => assertSidecars(directory, target)).toThrow('deno');
    } finally {
      fs.rmSync(directory, { recursive: true, force: true });
    }
  });
});
