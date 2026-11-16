import { spawn, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { assertSidecars, requiredSidecars } from './sidecars.mjs';

function quoteCmd(value) {
  return `"${String(value).replaceAll('"', '""')}"`;
}

/** Run a packaged helper. Windows CreateProcess runs `ffmpeg.exe` directly.
 * A `.cmd` launcher (used by the unit fake) goes through cmd.exe. */
function spawnPackaged(program, args) {
  const options = { encoding: 'utf8', timeout: 120_000, windowsHide: true };
  if (process.platform === 'win32' && /\.(?:cmd|bat)$/i.test(program)) {
    const command = [quoteCmd(program), ...args.map(quoteCmd)].join(' ');
    return spawnSync(process.env.ComSpec || 'cmd.exe', ['/d', '/s', '/c', `"${command}"`], {
      ...options,
      windowsVerbatimArguments: true,
    });
  }
  return spawnSync(program, args, options);
}

/** Describe a helper that did not run. A missing DLL exits 0xC0000135 with empty stderr. */
export function formatSpawnFailure(program, result) {
  const status =
    result.status === null || result.status === undefined
      ? 'none'
      : `0x${(result.status >>> 0).toString(16)}`;
  const signal = result.signal ?? 'none';
  const stderr = (result.stderr ?? '').toString().trim() || '(empty)';
  const spawn = result.error ? `, spawn ${result.error.message}` : '';
  return `Packaged helper failed: ${program}: exit ${status}, signal ${signal}, stderr ${stderr}${spawn}`;
}

/** The packaged ffmpeg must be able to encode MP3. yt-dlp asks for this encoder by name. */
export function assertLibmp3lameEncoder(program) {
  const result = spawnPackaged(program, ['-hide_banner', '-encoders']);
  if (result.error || result.status === null) {
    throw new Error(`Packaged ffmpeg could not be started: ${formatSpawnFailure(program, result)}`);
  }
  const text = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
  if (result.status !== 0 || !/(^|\s)libmp3lame(\s|$)/m.test(text)) {
    throw new Error(`Packaged ffmpeg is missing the libmp3lame encoder: ${program}`);
  }
}

/**
 * Windows command line of the packaged player. Mirrors `mpv_args` in
 * `src-tauri/src/player/engine.rs` plus the named pipe `--input-ipc-server`
 * added by `crates/mpv-ipc`. `ytdl-format` and `ytdl-raw-options` are absent:
 * they exist only in an mpv built with Lua, and the Windows build disables Lua.
 * An unknown option makes mpv exit 1 before the pipe exists.
 */
export function windowsPlaybackArgs(ytdlp, pipe) {
  const args = [
    '--idle=yes',
    '--no-config',
    '--load-scripts=no',
    '--no-terminal',
    '--no-video',
    '--audio-display=no',
    '--input-default-bindings=no',
    '--input-media-keys=no',
    '--audio-client-name=Vitra',
    '--prefetch-playlist=yes',
    '--media-controls=no',
  ];
  if (ytdlp) args.push(`--script-opts-append=ytdl_hook-ytdl_path=${ytdlp}`);
  args.push(`--input-ipc-server=${pipe}`);
  return args;
}

function spawnEngine(program, args) {
  const options = { windowsHide: true, stdio: 'ignore' };
  if (process.platform === 'win32' && /\.(?:cmd|bat)$/i.test(program)) {
    const command = [quoteCmd(program), ...args.map(quoteCmd)].join(' ');
    return spawn(process.env.ComSpec || 'cmd.exe', ['/d', '/s', '/c', command], {
      ...options,
      windowsVerbatimArguments: true,
    });
  }
  return spawn(program, args, options);
}

function connectIpc(endpoint) {
  return new Promise((resolve) => {
    const socket = net.connect(endpoint);
    const finish = (up) => {
      socket.removeAllListeners();
      socket.destroy();
      resolve(up);
    };
    socket.once('connect', () => finish(true));
    socket.once('error', () => finish(false));
  });
}

function stopEngine(child) {
  if (!child.pid || child.exitCode !== null) return;
  if (process.platform === 'win32') {
    spawnSync('taskkill', ['/pid', String(child.pid), '/t', '/f'], { windowsHide: true });
    return;
  }
  child.kill('SIGKILL');
}

/**
 * Start the packaged mpv the way Vitra does and wait until its IPC server accepts
 * a connection. `endpoint` overrides the named pipe so a unit fake can listen on
 * a Unix socket.
 */
export async function assertWindowsPlaybackIpc(program, ytdlp, endpoint) {
  const pipe = endpoint ?? `\\\\.\\pipe\\vitra-mpv0-${process.pid}`;
  const args = windowsPlaybackArgs(ytdlp, pipe);
  const luaOnly = args.filter(
    (arg) => arg.startsWith('--ytdl') || arg.includes('ytdl-raw-options'),
  );
  if (luaOnly.length) {
    throw new Error(`Lua-only mpv option on the command line: ${luaOnly.join(' ')}`);
  }
  const child = spawnEngine(program, args);
  const deadline = Date.now() + 10_000;
  try {
    while (Date.now() < deadline) {
      if (child.exitCode !== null) {
        throw new Error(`mpv exited before its IPC server came up (exit code: ${child.exitCode})`);
      }
      if (await connectIpc(pipe)) return;
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    throw new Error('mpv IPC pipe never came up');
  } finally {
    stopEngine(child);
  }
}

/** Find the resource directory in an extracted/installed package, never in the source tree. */
export function findSidecars(root, target) {
  const found = [];
  function visit(directory) {
    if (path.basename(directory) === 'sidecars') {
      try {
        assertSidecars(directory, target);
        found.push(directory);
      } catch {
        /* A partial candidate isn't a package. */
      }
    }
    for (const item of fs.readdirSync(directory, { withFileTypes: true })) {
      if (item.isDirectory() && !item.isSymbolicLink()) visit(path.join(directory, item.name));
    }
  }
  visit(root);
  if (found.length !== 1)
    throw new Error(`Expected one complete packaged sidecar directory, found ${found.length}`);
  return found[0];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [root, target] = process.argv.slice(2);
  if (!root || !target)
    throw new Error('Usage: node scripts/release/verify-sidecars.mjs PACKAGE_ROOT TARGET');
  const directory = findSidecars(path.resolve(root), target);
  for (const program of requiredSidecars(target)) {
    const ffmpeg = /^ff(mpeg|probe)/.test(path.basename(program));
    const result = spawnPackaged(path.join(directory, program), [
      ffmpeg ? '-version' : '--version',
    ]);
    if (result.error || result.status !== 0) throw new Error(formatSpawnFailure(program, result));
    const base = path.basename(program);
    if (base === 'ffmpeg' || base === 'ffmpeg.exe')
      assertLibmp3lameEncoder(path.join(directory, program));
    if (base === 'mpv.exe') {
      const ytdlp = path.join(directory, 'yt-dlp.exe');
      await assertWindowsPlaybackIpc(
        path.join(directory, program),
        fs.existsSync(ytdlp) ? ytdlp : undefined,
      );
      console.log(`${program}: IPC server accepted the player connection`);
    }
    console.log(`${program}: ${(result.stdout || result.stderr).split('\n')[0]}`);
  }
}
