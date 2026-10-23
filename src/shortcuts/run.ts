import { toggleLyrics } from '../features/lyrics/actions.ts';
import {
  focusSearch,
  nextSidebarMode,
  setCustomizing,
  setSidebarMode,
  sidebarMode,
  toggleModule,
  useCustomizing,
} from '../layout/state.ts';
import { mini } from '../lib/mini.ts';
import { pickAndPlay } from '../lib/openFiles.ts';
import { player } from '../lib/player.ts';
import { nextRepeat, queue } from '../lib/queue.ts';
import { useNavigation } from '../stores/navigation.ts';
import { usePlayer } from '../stores/player.ts';
import { useQueue } from '../stores/queue.ts';
import { useSettings } from '../stores/settings.ts';
import { useUi } from '../stores/ui.ts';
import type { ActionId } from './actions.ts';

const SEEK_STEP_S = 5;
const LONG_SEEK_STEP_S = 15;
const VOLUME_STEP = 5;

const report = (err: unknown) => console.error('A shortcut failed', err);

/** Runs `promise` in the background, reporting a failure. */
const fire = (promise: Promise<unknown>) => void promise.catch(report);

/** Seeks by `seconds`; nothing to seek leaves the key alone (it may scroll a list). */
function seekBy(seconds: number): boolean {
  const { state } = usePlayer.getState();
  if (state.duration === null) return false;
  fire(player.seek(Math.max(0, state.position + seconds)));
  return true;
}

function changeVolume(delta: number): void {
  const { volume } = usePlayer.getState().state;
  fire(player.setVolume(Math.min(100, Math.max(0, volume + delta))));
}

/**
 * What each action does. `false` means it did nothing, so the key keeps its usual meaning
 * (Space with nothing to play, arrows with nothing to seek).
 */
export const RUN: Record<ActionId, () => boolean | void> = {
  'play-pause': () => {
    if (!usePlayer.getState().state.track) return false;
    fire(player.toggle());
  },
  next: () => fire(player.next()),
  previous: () => fire(player.previous()),
  'seek-forward': () => seekBy(SEEK_STEP_S),
  'seek-back': () => seekBy(-SEEK_STEP_S),
  'seek-forward-long': () => seekBy(LONG_SEEK_STEP_S),
  'seek-back-long': () => seekBy(-LONG_SEEK_STEP_S),
  'volume-up': () => changeVolume(VOLUME_STEP),
  'volume-down': () => changeVolume(-VOLUME_STEP),
  mute: () => fire(player.setMuted(!usePlayer.getState().state.muted)),
  shuffle: () => fire(queue.setShuffle(!useQueue.getState().queue.shuffle)),
  repeat: () => fire(queue.setRepeat(nextRepeat(useQueue.getState().queue.repeat))),
  'open-files': () => fire(pickAndPlay()),
  'open-link': () => useUi.getState().openLink(''),
  search: focusSearch,
  back: () => useNavigation.getState().goBack(),
  forward: () => useNavigation.getState().goForward(),
  sidebar: () =>
    setSidebarMode(nextSidebarMode(sidebarMode(useSettings.getState().settings.layout))),
  queue: () => toggleModule('queue'),
  lyrics: toggleLyrics,
  'customize-layout': () => setCustomizing(!useCustomizing.getState().on),
  'mini-player': () => fire(mini.toggle()),
  settings: () => useUi.getState().showSettings(!useUi.getState().settingsOpen),
};
