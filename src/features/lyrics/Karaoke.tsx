import { useCallback, useRef } from 'react';

import { wordProgress, type LyricsLine } from '../../lib/lyrics.ts';
import { lyricsClock, useFrames } from './clock.ts';
import styles from './Karaoke.module.css';

/**
 * The playing line with word timings: sung words light up, the current one fills in. Colors
 * come from `--lyrics-lit` and `--lyrics-dim` on an ancestor.
 */
export function Karaoke({
  line,
  end,
  offsetMs,
  className,
}: {
  line: LyricsLine;
  end: number;
  offsetMs: number;
  className?: string;
}) {
  const wordsRef = useRef<HTMLSpanElement>(null);

  // Updated outside React: a re-render per frame for one line isn't worth it.
  useFrames(
    useCallback(() => {
      const spans = wordsRef.current?.children;
      if (!spans) {
        return;
      }
      const { index, fraction } = wordProgress(line, end, lyricsClock(offsetMs));
      for (let i = 0; i < spans.length; i++) {
        const span = spans[i] as HTMLElement;
        const state = i < index ? 'sung' : i === index ? 'singing' : 'upcoming';
        if (span.dataset.state !== state) {
          span.dataset.state = state;
        }
        if (i === index) {
          // The soft edge of the fill runs from just before the word to just past it.
          span.style.setProperty('--fill', `${(fraction * 116 - 16).toFixed(1)}%`);
        }
      }
    }, [line, end, offsetMs]),
  );

  return (
    <span ref={wordsRef} className={className}>
      {line.words.map((word, index) => (
        <span key={index} className={styles.word} data-state="upcoming">
          {word.text}
        </span>
      ))}
    </span>
  );
}
