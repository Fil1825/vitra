import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { describe, expect, it } from 'vitest';

import { requiredSidecars } from './sidecars.mjs';
import {
  assertLibmp3lameEncoder,
  assertWindowsPlaybackIpc,
  findSidecars,
  formatSpawnFailure,
  windowsPlaybackArgs,
} from './verify-sidecars.mjs';

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

  it('starts Windows mpv with the player arguments and no Lua-only options', () => {
    const args = windowsPlaybackArgs(
      'C:\\Program Files\\Vitra\\yt-dlp.exe',
      '\\\\.\\pipe\\vitra-mpv0-1',
    );
    expect(args).toContain('--load-scripts=no');
    expect(args).toContain('--idle=yes');
    expect(args).toContain('--no-video');
    expect(args).toContain('--media-controls=no');
    expect(args).toContain(
      '--script-opts-append=ytdl_hook-ytdl_path=C:\\Program Files\\Vitra\\yt-dlp.exe',
    );
    expect(args).toContain('--input-ipc-server=\\\\.\\pipe\\vitra-mpv0-1');
    expect(args.some((arg) => arg.startsWith('--ytdl') || arg.includes('ytdl-raw-options'))).toBe(
      false,
    );
  });

  it('waits for the IPC server and reports an early exit', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'vitra-mpv-ipc-'));
    const endpoint = (name) =>
      process.platform === 'win32'
        ? `\\\\.\\pipe\\vitra-mpv-test-${process.pid}-${name}`
        : path.join(root, `ipc-${name}.sock`);
    // node.exe is spawned directly. A .cmd launcher goes through cmd.exe, which
    // rewrites the named-pipe argument before the fake can listen on it.
    const script = (name, source) => {
      const file = path.join(root, `${name}.mjs`);
      fs.writeFileSync(file, source);
      return file;
    };
    const ready = script(
      'mpv',
      `import net from 'node:net';
const ipc = process.argv.find((arg) => arg.startsWith('--input-ipc-server='));
if (!ipc || process.argv.some((arg) => arg.startsWith('--ytdl') || arg.includes('ytdl-raw-options'))) {
  process.exit(1);
}
const server = net.createServer((socket) => socket.end());
server.on('error', () => process.exit(2));
server.listen(ipc.slice('--input-ipc-server='.length));
setInterval(() => {}, 1000);
`,
    );
    const refuse = script('mpv-refuse', 'process.exit(1);\n');
    try {
      await expect(
        assertWindowsPlaybackIpc(process.execPath, undefined, endpoint('ready'), [ready]),
      ).resolves.toBeUndefined();
      await expect(
        assertWindowsPlaybackIpc(process.execPath, undefined, endpoint('refuse'), [refuse]),
      ).rejects.toThrow(/mpv exited before its IPC server came up \(exit code: 1\)/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
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
