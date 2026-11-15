import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { requiredSidecars } from './sidecars.mjs';
import { assertLibmp3lameEncoder, findSidecars, formatSpawnFailure } from './verify-sidecars.mjs';

describe('installed package verification', () => {
  it('rejects missing and ambiguous packages, not just a successful build command', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-installed-'));
    const target = 'x86_64-unknown-linux-gnu';
    try {
      expect(() => findSidecars(root, target)).toThrow('found 0');
      for (const location of ['usr/lib/vitra', 'duplicate']) {
        const resources = path.join(root, location, 'sidecars');
        fs.mkdirSync(resources, { recursive: true });
        for (const name of requiredSidecars(target))
          fs.writeFileSync(path.join(resources, name), 'test program');
        if (location === 'usr/lib/vitra') expect(findSidecars(root, target)).toBe(resources);
      }
      expect(() => findSidecars(root, target)).toThrow('found 2');
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it('accepts a packaged ffmpeg that lists libmp3lame and rejects one that does not', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-ffmpeg-'));
    const script = path.join(root, 'ffmpeg-fake.mjs');
    // Windows does not run an extensionless `#!/bin/sh` file. The fake is a
    // Node script. Unix executes it through its shebang; Windows through a
    // .cmd that calls this Node. The packaged binary itself is ffmpeg.exe.
    const write = (source) => {
      fs.writeFileSync(script, source);
      if (process.platform === 'win32') {
        const program = path.join(root, 'ffmpeg.cmd');
        fs.writeFileSync(program, `@echo off\r\n"${process.execPath}" "${script}" %*\r\n`);
        return program;
      }
      const program = path.join(root, 'ffmpeg');
      fs.writeFileSync(program, `#!${process.execPath}\n${source}`);
      fs.chmodSync(program, 0o755);
      return program;
    };
    try {
      const listed = write(`if (process.argv.includes('-encoders')) {
  console.log(' A....D libmp3lame           libmp3lame MP3');
  process.exit(0);
}
process.exit(1);
`);
      expect(() => assertLibmp3lameEncoder(listed)).not.toThrow();
      const missing = write(`console.log(' A....D aac');
process.exit(0);
`);
      expect(() => assertLibmp3lameEncoder(missing)).toThrow(
        /Packaged ffmpeg is missing the libmp3lame encoder/,
      );
      const absent = path.join(root, 'no-such-ffmpeg');
      expect(() => assertLibmp3lameEncoder(absent)).toThrow(/could not be started/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  it('checks the Windows package binary as ffmpeg.exe', () => {
    expect(requiredSidecars('x86_64-pc-windows-msvc')).toContain('ffmpeg.exe');
  });

  it('reports a Windows loader abort with its exit code', () => {
    const text = formatSpawnFailure('mpv/mpv.exe', {
      status: 0xc0000135 | 0,
      signal: null,
      stderr: '',
      error: null,
    });
    // spawnSync reports STATUS_DLL_NOT_FOUND as a signed 32-bit status.
    expect(text).toContain('exit 0xc0000135');
    expect(text).toContain('signal none');
    expect(text).toContain('stderr (empty)');
  });
});
