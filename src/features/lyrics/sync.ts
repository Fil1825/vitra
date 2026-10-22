import type { Lyrics, LyricsLine } from '../../lib/lyrics.ts';

/** One tap, so it can be undone. */
interface Step {
  index: number;
  previous: LyricsLine;
}

/** Tap-to-sync: the lines, the line the next tap times, and the taps so far. */
export interface SyncState {
  lines: LyricsLine[];
  cursor: number;
  history: Step[];
}

const blank = (line: LyricsLine | undefined) => line !== undefined && line.text.trim() === '';

/** The next line worth a tap after `from`: blank lines (verse breaks) are skipped. */
export function nextLine(lines: readonly LyricsLine[], from: number): number {
  let index = from + 1;
  while (index < lines.length && blank(lines[index])) {
    index++;
  }
  return index;
}

/** Starts syncing at the first line without a time (or at the top when all have one). */
export function startSync(lines: LyricsLine[]): SyncState {
  const untimed = lines.findIndex((line) => line.start === null && !blank(line));
  return { lines, cursor: untimed >= 0 ? untimed : nextLine(lines, -1), history: [] };
}

/** Times the cursor's line at `ms` and moves on. Word timings move along with the line. */
export function tap(state: SyncState, ms: number): SyncState {
  const { lines, cursor } = state;
  const line = lines[cursor];
  if (!line) {
    return state;
  }
  const start = Math.max(0, Math.round(ms));
  const delta = line.start === null ? 0 : start - line.start;
  const timed: LyricsLine = {
    ...line,
    start,
    words:
      line.start === null
        ? []
        : line.words.map((word) => ({ ...word, start: Math.max(0, word.start + delta) })),
  };
  const next = lines.map((l, i) => (i === cursor ? timed : l));
  return {
    lines: next,
    cursor: nextLine(next, cursor),
    history: [...state.history, { index: cursor, previous: line }],
  };
}

/** Takes the last tap back. `seekTo` is a little before it, to tap it again. */
export function undo(state: SyncState): { state: SyncState; seekTo: number | null } {
  const step = state.history.at(-1);
  if (!step) {
    return { state, seekTo: null };
  }
  const tapped = state.lines[step.index]?.start ?? null;
  return {
    state: {
      lines: state.lines.map((line, i) => (i === step.index ? step.previous : line)),
      cursor: step.index,
      history: state.history.slice(0, -1),
    },
    seekTo: tapped === null ? null : Math.max(0, tapped - 3000),
  };
}

/** Moves every time by `deltaMs` (later when positive), stopping at zero. */
export function shiftAll(lines: readonly LyricsLine[], deltaMs: number): LyricsLine[] {
  const move = (ms: number) => Math.max(0, ms + deltaMs);
  return lines.map((line) => ({
    ...line,
    start: line.start === null ? null : move(line.start),
    words: line.words.map((word) => ({ ...word, start: move(word.start) })),
  }));
}

/**
 * The lyrics to save from the sync view. Once any line has a time the lyrics are LRC, which
 * keeps only timed lines: `untimed` counts the lines that would be left out.
 */
export function syncedLyrics(
  lines: readonly LyricsLine[],
  tags: [string, string][],
): { lyrics: Lyrics; untimed: number } {
  const synced = lines.some((line) => line.start !== null);
  if (!synced) {
    return { lyrics: { lines: [...lines], synced, offsetMs: 0, tags }, untimed: 0 };
  }
  const kept = lines.filter((line) => line.start !== null);
  const untimed = lines.filter((line) => line.start === null && !blank(line)).length;
  return { lyrics: { lines: kept, synced, offsetMs: 0, tags }, untimed };
}
