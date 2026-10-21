import { isTauri } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { create } from 'zustand';

import { bus } from '../lib/bus.ts';
import { INITIAL_PLAYER_STATE, player, type PlayerState } from '../lib/player.ts';

interface PlayerStore {
  state: PlayerState;
  /** `performance.now()` when `state.position` was reported. */
  positionAt: number;
}

/** Mirror of the backend player; only the backend changes it (through events). */
export const usePlayer = create<PlayerStore>(() => ({
  state: INITIAL_PLAYER_STATE,
  positionAt: 0,
}));

export async function initPlayer(): Promise<void> {
  bus.on('player:state', (state) => usePlayer.setState({ state, positionAt: performance.now() }));
  bus.on('player:position', ({ position, duration }) =>
    usePlayer.setState((s) => ({
      state: { ...s.state, position, duration },
      positionAt: performance.now(),
    })),
  );
  if (isTauri()) {
    try {
      usePlayer.setState({ state: await player.state(), positionAt: performance.now() });
    } catch (err) {
      console.error('Reading the player state failed', err);
    }
  }
}

/**
 * The position extrapolated from the last report while playing: the backend sends a few
 * updates per second and the UI fills in between.
 */
export function livePosition(state: PlayerState, positionAt: number, now: number): number {
  if (state.status !== 'playing') {
    return state.position;
  }
  const position = state.position + (Math.max(0, now - positionAt) / 1000) * state.speed;
  return state.duration === null ? position : Math.min(position, state.duration);
}

/** Smoothly updating playback position for progress bars and time labels. */
export function useLivePosition(intervalMs = 100): number {
  const { state, positionAt } = usePlayer();
  const [now, setNow] = useState(() => performance.now());
  const playing = state.status === 'playing';

  useEffect(() => {
    if (!playing) {
      return;
    }
    const timer = window.setInterval(() => setNow(performance.now()), intervalMs);
    return () => window.clearInterval(timer);
  }, [playing, intervalMs]);

  return livePosition(state, positionAt, Math.max(now, positionAt));
}
