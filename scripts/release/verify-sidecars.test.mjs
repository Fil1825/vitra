import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { requiredSidecars } from './sidecars.mjs';
import { assertLibmp3lameEncoder, findSidecars } from './verify-sidecars.mjs';

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
    const program = path.join(root, 'ffmpeg');
    const write = (body) => {
      fs.writeFileSync(program, body);
      fs.chmodSync(program, 0o755);
    };
    try {
      write(`#!/bin/sh
for arg in "$@"; do
  if [ "$arg" = "-encoders" ]; then
    echo " A....D libmp3lame           libmp3lame MP3"
    exit 0
  fi
done
exit 1
`);
      expect(() => assertLibmp3lameEncoder(program)).not.toThrow();
      write(`#!/bin/sh
echo " A....D aac"
exit 0
`);
      expect(() => assertLibmp3lameEncoder(program)).toThrow(/libmp3lame/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
