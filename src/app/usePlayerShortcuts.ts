import { useEffect } from 'react';

import { pickAndPlay } from '../lib/openFiles.ts';
import { player } from '../lib/player.ts';
import { usePlayer } from '../stores/player.ts';

const SEEK_STEP_S = 5;
const VOLUME_STEP = 5;

/** Keys that would also activate a focused control; those keep their native behavior. */
function isInteractive(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable ||
      target.closest('button, input, select, textarea, a[href], [role="menu"]') !== null)
  );
}

const report = (err: unknown) => console.error('Player shortcut failed', err);

/** Basic playback keys; rebindable hotkeys come in stage 7. */
export function usePlayerShortcuts(): void {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;
      if (command && event.key.toLowerCase() === 'o') {
        event.preventDefault();
        pickAndPlay().catch(report);
        return;
      }
      if (isInteractive(event.target) || event.altKey) {
        return;
      }
      const { state } = usePlayer.getState();
      switch (event.key) {
        case ' ':
          event.preventDefault();
          if (state.track) player.toggle().catch(report);
          break;
        case 'ArrowLeft':
        case 'ArrowRight': {
          // Cmd/Ctrl+←/→: previous / next track, like other players.
          if (command) {
            event.preventDefault();
            (event.key === 'ArrowLeft' ? player.previous() : player.next()).catch(report);
            return;
          }
          if (state.duration === null) return;
          event.preventDefault();
          const delta =
            (event.key === 'ArrowLeft' ? -1 : 1) * SEEK_STEP_S * (event.shiftKey ? 3 : 1);
          player.seek(Math.max(0, state.position + delta)).catch(report);
          break;
        }
        case 'ArrowUp':
        case 'ArrowDown': {
          if (!command) return;
          event.preventDefault();
          const delta = event.key === 'ArrowUp' ? VOLUME_STEP : -VOLUME_STEP;
          player.setVolume(Math.min(100, Math.max(0, state.volume + delta))).catch(report);
          break;
        }
        case 'm':
        case 'M':
          if (!command) player.setMuted(!state.muted).catch(report);
          break;
        default:
          break;
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);
}
