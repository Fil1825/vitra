import { useEffect } from 'react';

import { livePosition, usePlayer } from '../../stores/player.ts';

/** The playing position in lyrics time (milliseconds), read fresh on every call. */
export function lyricsClock(offsetMs: number): number {
  const { state, positionAt } = usePlayer.getState();
  return livePosition(state, positionAt, performance.now()) * 1000 - offsetMs;
}

/** Calls `tick` every frame while playing, and on every player update while paused. */
export function useFrames(tick: () => void): void {
  const playing = usePlayer((s) => s.state.status === 'playing');
  useEffect(() => {
    let frame = 0;
    const loop = () => {
      tick();
      frame = requestAnimationFrame(loop);
    };
    if (playing) {
      loop();
      return () => cancelAnimationFrame(frame);
    }
    tick();
    // Seeks while paused.
    return usePlayer.subscribe(tick);
  }, [playing, tick]);
}
