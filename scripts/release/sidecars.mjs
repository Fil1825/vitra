import fs from 'node:fs';
import path from 'node:path';

import { SUPPORTED_TARGETS, exeName, YT_DLP_MACOS } from '../sidecars/plan.mjs';

/** Programs that must be inside a distributable package. Linux declares system mpv. */
export function requiredSidecars(target) {
  if (!SUPPORTED_TARGETS.includes(target)) throw new Error(`Unsupported release target: ${target}`);
  const mac = target.endsWith('apple-darwin');
  const linux = target.includes('linux');
  return [
    ...(!linux ? [mac ? 'mpv/mpv.app/Contents/MacOS/mpv' : 'mpv/mpv.exe'] : []),
    mac ? YT_DLP_MACOS : exeName(target, 'yt-dlp'),
    ...['ffmpeg', 'ffprobe', 'deno'].map((name) => exeName(target, name)),
  ];
}

export function assertSidecars(directory, target) {
  const missing = requiredSidecars(target).filter(
    (name) => !fs.statSync(path.join(directory, name), { throwIfNoEntry: false })?.isFile(),
  );
  if (missing.length)
    throw new Error(
      `Incomplete sidecars for ${target}: ${missing.join(', ')}. Run npm run fetch-sidecars first.`,
    );
}
