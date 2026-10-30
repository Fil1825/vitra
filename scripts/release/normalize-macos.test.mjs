import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { normalizePythonFramework } from './normalize-macos.mjs';

// This repair produces POSIX framework symlinks and runs only in the macOS signing job.
describe.skipIf(process.platform === 'win32')('flattened Python frameworks', () => {
  function fixture() {
    const sidecars = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-framework-'));
    const root = path.join(sidecars, 'yt-dlp', '_internal', 'Python.framework');
    const version = path.join(root, 'Versions', '3.14');
    fs.mkdirSync(path.join(version, 'Resources'), { recursive: true });
    fs.writeFileSync(path.join(version, 'Python'), 'native code');
    fs.writeFileSync(path.join(version, 'Resources', 'Info.plist'), 'framework metadata');
    fs.cpSync(version, path.join(root, 'Versions', 'Current'), { recursive: true });
    fs.copyFileSync(path.join(version, 'Python'), path.join(root, 'Python'));
    fs.cpSync(path.join(version, 'Resources'), path.join(root, 'Resources'), { recursive: true });
    return { sidecars, root };
  }

  it('restores canonical aliases without changing the versioned payload', () => {
    const { sidecars, root } = fixture();
    try {
      normalizePythonFramework(sidecars);
      normalizePythonFramework(sidecars);
      expect(fs.readlinkSync(path.join(root, 'Python'))).toBe('Versions/Current/Python');
      expect(fs.readlinkSync(path.join(root, 'Versions', 'Current'))).toBe('3.14');
      expect(fs.readFileSync(path.join(root, 'Python'), 'utf8')).toBe('native code');
      expect(fs.readFileSync(path.join(root, 'Resources', 'Info.plist'), 'utf8')).toBe(
        'framework metadata',
      );
    } finally {
      fs.rmSync(sidecars, { recursive: true, force: true });
    }
  });

  it('refuses to remove differing files before modifying any alias', () => {
    const { sidecars, root } = fixture();
    try {
      fs.writeFileSync(path.join(root, 'Python'), 'different native code');
      expect(() => normalizePythonFramework(sidecars)).toThrow('differs');
      expect(fs.lstatSync(path.join(root, 'Versions', 'Current')).isSymbolicLink()).toBe(false);
      expect(fs.readFileSync(path.join(root, 'Python'), 'utf8')).toBe('different native code');
    } finally {
      fs.rmSync(sidecars, { recursive: true, force: true });
    }
  });
});
